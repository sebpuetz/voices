use async_trait::async_trait;
use tonic::Status;
use uuid::Uuid;
use voice_server::channel::connection::ConnectionState;

use super::VoiceHost;

#[derive(Clone)]
pub struct IntegratedVoiceHost {
    inner: voice_server::VoiceServerImpl,
}

impl IntegratedVoiceHost {
    pub fn new(inner: voice_server::VoiceServerImpl) -> Self {
        Self { inner }
    }

    pub async fn assign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
        self.inner.assign_channel_impl(channel_id).await?;
        Ok(())
    }
}

#[async_trait]
impl VoiceHost for IntegratedVoiceHost {
    async fn open_connection(
        &self,
        request: voice_server::OpenConnection,
    ) -> Result<voice_server::ConnectionData, Status> {
        Ok(self.inner.open_connection_impl(request).await?)
    }

    async fn establish_session(
        &self,
        request: voice_server::EstablishSession,
    ) -> Result<voice_server::SessionData, Status> {
        Ok(self.inner.establish_session_impl(request).await?)
    }
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), Status> {
        Ok(self.inner.leave_impl(channel_id, client_id).await?)
    }

    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, Status> {
        Ok(self.inner.user_status_impl(channel_id, client_id).await?)
    }
    async fn status(&self, channel_id: Uuid) -> Result<Vec<voice_server::Peer>, Status> {
        Ok(self.inner.status_impl(channel_id).await?)
    }
}
