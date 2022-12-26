pub mod config;
pub mod connection;
pub mod grpc;
mod ports;
use connection::{StatusResponse, VoiceTask};
use futures_util::stream::FuturesUnordered;
pub use ports::Ports;
use tokio_stream::StreamExt;
use tracing::Instrument;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::connection::{ConnectionState, VoiceConnection, VoiceControl};
use crate::ports::PortRef;

pub use grpc::proto::voice_server_server::VoiceServer;

#[derive(Clone)]
pub struct VoiceServerImpl {
    host_addr: IpAddr,
    ports: Ports,
    rooms: Arc<RwLock<HashMap<Uuid, Room>>>,
}

impl VoiceServerImpl {
    pub fn new(host_addr: IpAddr, ports: Ports) -> Self {
        Self {
            host_addr,
            ports,
            rooms: Arc::default(),
        }
    }

    /// Get the room identified by [`id`].
    // FIXME: Currently creates the room if it doesn't exist, replace with channel registry.
    pub async fn room(&self, id: Uuid) -> Room {
        self.rooms
            .write()
            .await
            .entry(id)
            .or_insert_with(|| {
                let (voice_tx, _) = broadcast::channel(3);
                Room::new(id, voice_tx)
            })
            .clone()
    }

    /// Try to reserve a port for a voice connection.
    pub async fn allocate_port(&self) -> Option<PortRef> {
        self.ports.get()
    }

    /// Start establishing a voice connection.
    #[tracing::instrument(skip_all, fields(?req.client_id, channel_id=?req.channel_id))]
    pub async fn open_connection_impl(
        &self,
        req: OpenConnection,
    ) -> Result<ConnectionData, OpenConnectionError> {
        let room = self.room(req.channel_id).await;

        let port_ref = self
            .allocate_port()
            .await
            .ok_or(OpenConnectionError::NoOpenPorts)?;
        let port = port_ref.port;
        match room.join(req.client_id, port_ref, self.host_addr).await {
            Ok(()) => {
                tracing::info!("joined channel");
                Ok(ConnectionData {
                    sock: SocketAddr::from((self.host_addr, port)),
                })
            }
            Err(JoinError::ConnectionAlreadyOpen(sock)) => {
                tracing::warn!("client was already present");
                Ok(ConnectionData { sock })
            }
            Err(JoinError::Other(inner)) => {
                tracing::warn!(error =?inner, "failed to join channel");
                Err(OpenConnectionError::Other(inner))
            }
        }
    }

    pub async fn establish_session_impl(
        &self,
        req: EstablishSession,
    ) -> Result<SessionData, EstablishSessionError> {
        let room = self.room(req.channel_id).await;
        let ctl = room.peer(req.client_id).await?;
        let response = ctl.init(req.client_addr).await?;
        Ok(SessionData {
            src_id: response.source_id,
            crypt_key: response.crypt_key,
        })
    }
}

pub struct OpenConnection {
    pub channel_id: Uuid,
    pub client_id: Uuid,
}

pub struct ConnectionData {
    pub sock: SocketAddr,
}

#[derive(thiserror::Error, Debug)]
pub enum OpenConnectionError {
    #[error("no open ports left")]
    NoOpenPorts,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub struct EstablishSession {
    pub channel_id: Uuid,
    pub client_id: Uuid,
    pub client_addr: SocketAddr,
}

pub struct SessionData {
    pub src_id: u32,
    pub crypt_key: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum EstablishSessionError {
    #[error(transparent)]
    PeerNotFound(#[from] PeerNotFound),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("peer not found")]
pub struct PeerNotFound;

#[derive(thiserror::Error, Debug)]
pub enum JoinError {
    #[error("connection already open")]
    ConnectionAlreadyOpen(SocketAddr),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct Room {
    voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>,
    peers: Arc<RwLock<HashMap<Uuid, VoiceControl>>>,
    handles_tx: mpsc::Sender<VoiceTask>,
    span: tracing::Span,
}

impl Room {
    pub fn new(id: Uuid, voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>) -> Self {
        let span = tracing::info_span!("room", id=?id);
        span.follows_from(None);
        let peers: Arc<RwLock<HashMap<Uuid, VoiceControl>>> = Arc::default();
        let peers_clone = peers.clone();
        let (handles_tx, mut handles_rx) = mpsc::channel(10);
        let watcher_span = tracing::debug_span!("voice_task_watcher");
        watcher_span.follows_from(&span);
        tokio::spawn(
            async move {
                let mut handles = FuturesUnordered::new();
                loop {
                    tokio::select! {
                        Some(id) = handles.next() => {
                            tracing::debug!("task for client_id={id} ended");
                            peers_clone.write().await.remove(&id);
                        },
                        task = handles_rx.recv() => {
                            match task {
                                Some(task) => {
                                    tracing::debug!("received task for client_id={id}");
                                    handles.push(task);
                                },
                                None => break,
                            }
                        }
                    }
                }
            }
            .instrument(watcher_span),
        );
        Self {
            voice_tx,
            peers,
            handles_tx,
            span,
        }
    }

    #[tracing::instrument(follows_from = [self.span.id()], skip_all)]
    pub async fn join(
        &self,
        client_id: Uuid,
        port: PortRef,
        host_ip: IpAddr,
    ) -> Result<(), JoinError> {
        match self.peers.write().await.entry(client_id) {
            std::collections::hash_map::Entry::Occupied(mut occ) => {
                tracing::info!("client is already present");
                match occ.get().status().await {
                    Ok(StatusResponse {
                        state: ConnectionState::Waiting,
                        udp_addr,
                    }) => Err(JoinError::ConnectionAlreadyOpen(udp_addr)),
                    Ok(StatusResponse {
                        state: ConnectionState::Peered,
                        udp_addr: _,
                    }) => Err(JoinError::Other(anyhow::anyhow!(
                        "connection already initialized"
                    ))),
                    Err(_) => {
                        let (control, task) =
                            VoiceConnection::start(client_id, port, self.voice_tx.clone(), host_ip)
                                .await?;
                        occ.insert(control);
                        self.handles_tx
                            .send(task)
                            .await
                            .map_err(anyhow::Error::from)?;
                        Ok(())
                    }
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                let (control, task) =
                    VoiceConnection::start(client_id, port, self.voice_tx.clone(), host_ip).await?;
                v.insert(control);
                self.handles_tx
                    .send(task)
                    .await
                    .map_err(anyhow::Error::from)?;
                tracing::debug!("stored voice control");
                Ok(())
            }
        }
    }

    pub async fn peer(&self, client_id: Uuid) -> Result<VoiceControl, PeerNotFound> {
        self.peers
            .read()
            .await
            .get(&client_id)
            .cloned()
            .ok_or(PeerNotFound)
    }

    pub async fn status(&self, client_id: Uuid) -> StatusResponse {
        let ctl = self.peer(client_id).await.expect("FIXME");
        ctl.status().await.expect("FIXME")
    }

    pub async fn leave(&self, client_id: Uuid) -> Option<()> {
        let ctl = {
            let mut peers = self.peers.write().await;
            peers.remove(&client_id)?
        };
        ctl.stop().await;
        Some(())
    }
}
