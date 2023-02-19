use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use voices_channels::grpc::proto::channels_server::Channels;
use voices_channels::grpc::proto::{RegisterVoiceServerRequest, UnassignChannelRequest};

#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait Register: Send + Sync + 'static {
    async fn register(&self, addr: String) -> anyhow::Result<()>;
    async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()>;
}

#[async_trait]
impl Register for Registry {
    async fn register(&self, addr: String) -> anyhow::Result<()> {
        let request = tonic::Request::new(RegisterVoiceServerRequest {
            id: self.server_id.to_string(),
            addr,
        });
        self.inner.register_voice_server(request).await?;
        Ok(())
    }
    async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
        let message = UnassignChannelRequest {
            channel_id: channel_id.to_string(),
            server_id: self.server_id.to_string(),
        };
        let request = tonic::Request::new(message);
        self.inner.unassign_channel(request).await.map_err(|e| {
            tracing::warn!("failed to unassign self from channel {}", e);
            e
        })?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Registry {
    inner: Arc<dyn Channels>,
    server_id: Uuid,
}

impl Registry {
    pub fn new(inner: Arc<dyn Channels>, server_id: Uuid) -> Self {
        Self { inner, server_id }
    }
}
