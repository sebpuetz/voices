pub mod connection;
pub mod grpc;
mod ports;
use connection::StatusResponse;
pub use ports::Ports;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::connection::{ConnectionState, VoiceConnection, VoiceControl};
use crate::ports::PortRef;

#[derive(Clone)]
pub struct VoiceServer {
    host_addr: IpAddr,
    ports: Ports,
    rooms: Arc<RwLock<HashMap<Uuid, Room>>>,
}

impl VoiceServer {
    pub fn new(host_addr: IpAddr, ports: Ports) -> Self {
        Self {
            host_addr,
            ports,
            rooms: Arc::default(),
        }
    }

    pub async fn room(&self, id: Uuid) -> Room {
        self.rooms
            .write()
            .await
            .entry(id)
            // FIXME: implement persistent channel registry
            .or_insert_with(|| {
                let (voice_tx, _) = broadcast::channel(50);
                Room {
                    id,
                    voice_tx,
                    peers: Arc::default(),
                }
            })
            .clone()
    }

    pub async fn allocate_port(&self) -> Option<PortRef> {
        self.ports.get()
    }

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

    pub fn host_addr(&self) -> IpAddr {
        self.host_addr
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
    id: Uuid,
    voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>,
    peers: Arc<RwLock<HashMap<Uuid, VoiceControl>>>,
}

impl Room {
    #[tracing::instrument(skip_all, fields(?client_id, id=?self.id))]
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
                        let control =
                            VoiceConnection::start(client_id, port, self.voice_tx.clone(), host_ip)
                                .await?;
                        occ.insert(control);
                        Ok(())
                    }
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                let control =
                    VoiceConnection::start(client_id, port, self.voice_tx.clone(), host_ip).await?;
                v.insert(control);
                tracing::debug!("stored voice control");
                Ok(())
            }
        }
    }

    pub async fn peer(&self, client_id: Uuid) -> Result<VoiceControl, PeerNotFound> {
        dbg!(self.peers.read().await)
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
