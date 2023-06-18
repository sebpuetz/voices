pub mod channel;
pub mod config;
pub use ports::Ports;
pub use voices_voice_models::*;

mod ports;

use anyhow::Context;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::channel::Channel;
use crate::ports::PortRef;

#[derive(Clone)]
pub struct VoiceServerImpl {
    host_addr: IpAddr,
    ports: Ports,
    channels: Arc<RwLock<HashMap<Uuid, Channel>>>,
    channels_cleanup_tx: mpsc::Sender<Uuid>,
}

pub trait ChannelEndNotify: Send + Sync + 'static {
    fn notify(&self, channel_id: Uuid);
}

impl<F> ChannelEndNotify for F
where
    F: Fn(Uuid) + Send + Sync + 'static,
{
    fn notify(&self, channel_id: Uuid) {
        (self)(channel_id)
    }
}

impl VoiceServerImpl {
    pub fn new(
        host_addr: IpAddr,
        ports: Ports,
        room_end_notify: Option<Arc<dyn ChannelEndNotify>>,
    ) -> Self {
        let (room_tx, mut room_rx) = mpsc::channel(10);
        let rooms: Arc<RwLock<HashMap<Uuid, Channel>>> = Arc::default();
        let rooms_handle = rooms.clone();
        match room_end_notify {
            Some(notify) => {
                tokio::spawn(async move {
                    // cleanup task for stopped channels
                    while let Some(id) = room_rx.recv().await {
                        rooms_handle.write().await.remove(&id);
                        tracing::info!("removed channel from local registry {}", id);
                        notify.notify(id);
                    }
                    tracing::warn!("cleanup task stopped");
                });
            }
            None => {
                tokio::spawn(async move {
                    // cleanup task for stopped channels
                    while let Some(id) = room_rx.recv().await {
                        rooms_handle.write().await.remove(&id);
                        tracing::info!("dropped channel {}", id);
                    }
                });
            }
        };
        Self {
            host_addr,
            ports,
            channels: rooms,
            channels_cleanup_tx: room_tx,
        }
    }

    /// Get the room identified by [`id`].
    pub async fn assign_channel(&self, id: Uuid) -> anyhow::Result<Channel> {
        match self.channels.write().await.entry(id) {
            std::collections::hash_map::Entry::Occupied(o) => Ok(o.get().to_owned()),
            std::collections::hash_map::Entry::Vacant(v) => {
                // FIXME: specific error
                let port = self.allocate_port().context("ports exhaused")?;
                let c = Channel::new(id, self.channels_cleanup_tx.clone(), port).await?;
                Ok(v.insert(c).to_owned())
            }
        }
    }

    pub async fn get_channel(&self, id: Uuid) -> Option<Channel> {
        self.channels
            .read()
            .await
            .get(&id)
            .cloned()
            .filter(|c| c.healthy())
    }

    /// Try to reserve a port for a voice connection.
    pub fn allocate_port(&self) -> Option<PortRef> {
        self.ports.get()
    }

    /// Start establishing a voice connection.
    #[tracing::instrument(skip_all, fields(?req.client_id, channel_id=?req.channel_id))]
    pub async fn open_connection(
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

    pub async fn establish_session(
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

    pub async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), LeaveError> {
        let room = self.get_channel(channel_id).await.ok_or(ChannelNotFound)?;
        room.leave(client_id).await.ok_or(PeerNotFound)?;
        Ok(())
    }

    pub async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, StatusError> {
        let room = self.get_channel(channel_id).await.ok_or(ChannelNotFound)?;
        Ok(room
            .status(client_id)
            .await
            .map_err(|e| {
                tracing::warn!("voice status bad: {}", e);
                e
            })?
            .state)
    }

    pub async fn status(&self, channel_id: Uuid) -> Result<Vec<Peer>, StatusError> {
        let room = self.get_channel(channel_id).await.ok_or(ChannelNotFound)?;
        if !room.healthy() {
            // FIXME
            return Err(ChannelNotFound.into());
        }
        let peers = room.peers().await.map_err(|e| {
            tracing::warn!("failed to get peers: {}", e);
            anyhow::anyhow!("failed to get peers")
        })?;
        Ok(peers)
    }
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
pub enum LeaveError {
    #[error(transparent)]
    ChannelNotFound(#[from] ChannelNotFound),
    #[error(transparent)]
    PeerNotFound(#[from] PeerNotFound),
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
    ChannelNotFound(#[from] ChannelNotFound),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
#[derive(thiserror::Error, Debug)]
#[error("channel not hosted here")]
pub struct ChannelNotFound;

#[cfg(any(test, feature = "test-internals"))]
mod test;
