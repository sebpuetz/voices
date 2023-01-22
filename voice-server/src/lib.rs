pub mod channel;
pub mod config;
pub mod grpc;
mod ports;
use anyhow::Context;
pub use ports::Ports;
use tracing::Instrument;

use voices_channels::grpc::proto::channels_server::Channels;
use voices_channels::grpc::proto::UnassignChannelRequest;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::channel::Channel;
use crate::ports::PortRef;

pub use grpc::proto::voice_server_server::VoiceServer;

#[derive(Clone)]
pub struct VoiceServerImpl {
    host_addr: IpAddr,
    ports: Ports,
    rooms: Arc<RwLock<HashMap<Uuid, Channel>>>,
    room_tx: mpsc::Sender<Uuid>,
}

impl VoiceServerImpl {
    pub fn new(host_addr: IpAddr, ports: Ports, channels: Arc<dyn Channels>) -> Self {
        let (room_tx, mut room_rx) = mpsc::channel(10);
        let rooms: Arc<RwLock<HashMap<Uuid, Channel>>> = Arc::default();
        let rooms_handle = rooms.clone();
        tokio::spawn(async move {
            while let Some(id) = room_rx.recv().await {
                rooms_handle.write().await.remove(&id);
                let channels = channels.clone();
                tokio::spawn(
                    async move {
                        let message = UnassignChannelRequest {
                            channel_id: id.to_string(),
                        };
                        let request = tonic::Request::new(message);
                        channels
                            .clone()
                            .unassign_channel(request)
                            .await
                            .map_err(|e| {
                                tracing::warn!("failed to unassign self from channel {}", e);
                            })
                    }
                    .instrument(tracing::info_span!("unassign_server", channel_id=?id)),
                );
                tracing::info!("dropped channel {}", id);
            }
            tracing::warn!("cleanup task stopped");
        });
        Self {
            host_addr,
            ports,
            rooms,
            room_tx,
        }
    }

    /// Get the room identified by [`id`].
    pub async fn get_or_create_channel(&self, id: Uuid) -> anyhow::Result<Channel> {
        match self.rooms.write().await.entry(id) {
            std::collections::hash_map::Entry::Occupied(o) => Ok(o.get().to_owned()),
            std::collections::hash_map::Entry::Vacant(v) => {
                // FIXME: specific error
                let port = self.allocate_port().context("ports exhaused")?;
                let c = Channel::new(id, self.room_tx.clone(), port).await?;
                Ok(v.insert(c).to_owned())
            }
        }
    }

    pub async fn get_channel(&self, id: Uuid) -> Option<Channel> {
        self.rooms.read().await.get(&id).cloned()
    }

    /// Try to reserve a port for a voice connection.
    pub fn allocate_port(&self) -> Option<PortRef> {
        self.ports.get()
    }

    /// Start establishing a voice connection.
    #[tracing::instrument(skip_all, fields(?req.client_id, channel_id=?req.channel_id))]
    pub async fn open_connection_impl(
        &self,
        req: OpenConnection,
    ) -> Result<ConnectionData, OpenConnectionError> {
        let room = self
            .get_channel(req.channel_id)
            .await
            .ok_or(ChannelNotFound)?;

        match room.open_connection(req.client_id, req.user_name).await {
            Ok(source_id) => {
                tracing::info!("joined channel");
                Ok(ConnectionData {
                    source_id,
                    sock: SocketAddr::from((self.host_addr, room.port())),
                })
            }
            Err(e) => {
                tracing::warn!("failed to open connection");
                Err(e)
            }
        }
    }

    pub async fn establish_session_impl(
        &self,
        req: EstablishSession,
    ) -> Result<SessionData, EstablishSessionError> {
        let channel = self
            .get_channel(req.channel_id)
            .await
            .ok_or(ChannelNotFound)?;

        let response = channel
            .establish_session(req.client_id, req.client_addr)
            .await?;
        Ok(SessionData {
            crypt_key: response.crypt_key,
        })
    }
}

pub struct OpenConnection {
    pub user_name: String,
    pub channel_id: Uuid,
    pub client_id: Uuid,
}

pub struct ConnectionData {
    pub sock: SocketAddr,
    pub source_id: u32,
}

#[derive(thiserror::Error, Debug)]
pub enum OpenConnectionError {
    #[error(transparent)]
    ChannelNotFound(#[from] ChannelNotFound),
    #[error("no open ports left")]
    NoOpenPorts,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("channel not hosted here")]
pub struct ChannelNotFound;

pub struct EstablishSession {
    pub channel_id: Uuid,
    pub client_id: Uuid,
    pub client_addr: SocketAddr,
}

pub struct SessionData {
    pub crypt_key: Vec<u8>,
}

impl std::fmt::Debug for SessionData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionData")
            .field("crypt_key", &"CENSORED")
            .finish()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EstablishSessionError {
    #[error(transparent)]
    PeerNotFound(#[from] PeerNotFound),
    #[error(transparent)]
    ChannelNotFound(#[from] ChannelNotFound),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("peer not found")]
pub struct PeerNotFound;

#[derive(thiserror::Error, Debug)]
pub enum JoinError {
    #[error("connection already open")]
    ConnectionAlreadyOpen(SocketAddr, u32),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum StatusError {
    #[error(transparent)]
    PeerNotFound(#[from] PeerNotFound),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
