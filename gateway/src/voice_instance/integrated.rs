use async_trait::async_trait;
use uuid::Uuid;
use voices_voice::{
    EstablishSessionError, LeaveError, OpenConnectionError, StatusError, VoiceServerImpl,
};

use super::*;

#[derive(Clone)]
pub struct IntegratedVoiceHost {
    inner: VoiceServerImpl,
}

impl IntegratedVoiceHost {
    pub fn new(inner: VoiceServerImpl) -> Self {
        Self { inner }
    }

    pub async fn assign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
        self.inner.assign_channel(channel_id).await?;
        Ok(())
    }
}

#[async_trait]
impl VoiceHost for IntegratedVoiceHost {
    async fn open_connection(
        &self,
        request: OpenConnection,
    ) -> Result<ConnectionData, VoiceHostError> {
        Ok(self.inner.open_connection(request).await?)
    }

    async fn establish_session(
        &self,
        request: EstablishSession,
    ) -> Result<SessionData, VoiceHostError> {
        Ok(self.inner.establish_session(request).await?)
    }
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), VoiceHostError> {
        self.inner.leave(channel_id, client_id).await?;
        Ok(())
    }

    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, VoiceHostError> {
        Ok(self.inner.user_status(channel_id, client_id).await?)
    }
    async fn status(&self, channel_id: Uuid) -> Result<Vec<Peer>, VoiceHostError> {
        Ok(self.inner.status(channel_id).await?)
    }
}

impl From<OpenConnectionError> for VoiceHostError {
    fn from(value: OpenConnectionError) -> Self {
        match value {
            OpenConnectionError::ChannelNotFound(_) => VoiceHostError::NotFound("channel"),
            OpenConnectionError::NoOpenPorts => VoiceHostError::Other(
                anyhow::Error::from(value).context("failed to open connection"),
            ),
            OpenConnectionError::Other(o) => VoiceHostError::Other(o),
        }
    }
}

impl From<EstablishSessionError> for VoiceHostError {
    fn from(value: EstablishSessionError) -> Self {
        match value {
            EstablishSessionError::PeerNotFound(_) => VoiceHostError::NotFound("peer"),
            EstablishSessionError::ChannelNotFound(_) => VoiceHostError::NotFound("channel"),
            EstablishSessionError::Other(o) => VoiceHostError::Other(o),
        }
    }
}

impl From<LeaveError> for VoiceHostError {
    fn from(value: LeaveError) -> Self {
        match value {
            LeaveError::PeerNotFound(_) => VoiceHostError::NotFound("peer"),
            LeaveError::ChannelNotFound(_) => VoiceHostError::NotFound("channel"),
        }
    }
}

impl From<StatusError> for VoiceHostError {
    fn from(value: StatusError) -> Self {
        match value {
            StatusError::PeerNotFound(_) => VoiceHostError::NotFound("peer"),
            StatusError::ChannelNotFound(_) => VoiceHostError::NotFound("channel"),
            StatusError::Other(o) => VoiceHostError::Other(o),
        }
    }
}
