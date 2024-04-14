use std::sync::Arc;

use async_trait::async_trait;
use tracing::Instrument;
use uuid::Uuid;

#[path = "./voice_channels.v1.rs"]
pub mod voice_channels;
use voice_channels::{
    channels_client::ChannelsClient, RegisterVoiceServerRequest, UnassignChannelRequest,
};
use voices_voice::ChannelEndNotify;

#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait Register: Send + Sync + 'static {
    async fn register(&self, addr: String) -> anyhow::Result<()>;
    async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()>;
}

#[async_trait]
impl Register for RemoteRegistry {
    async fn register(&self, addr: String) -> anyhow::Result<()> {
        let request = tonic::Request::new(RegisterVoiceServerRequest {
            id: self.server_id.to_string(),
            addr,
        });
        self.inner.clone().register_voice_server(request).await?;
        Ok(())
    }

    async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
        let message = UnassignChannelRequest {
            channel_id: channel_id.to_string(),
            server_id: self.server_id.to_string(),
        };
        let request = tonic::Request::new(message);
        self.inner
            .clone()
            .unassign_channel(request)
            .await
            .map_err(|e| {
                tracing::warn!("failed to unassign self from channel {}", e);
                e
            })?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct RemoteRegistry {
    inner: ChannelsClient<tonic::transport::Channel>,
    server_id: Uuid,
}

#[derive(Clone)]
pub struct Registry(Arc<dyn Register>);

impl Registry {
    pub fn new_client(inner: ChannelsClient<tonic::transport::Channel>, server_id: Uuid) -> Self {
        Self::new(RemoteRegistry { inner, server_id })
    }

    pub fn new<R>(registry: R) -> Self
    where
        R: Register,
    {
        Self(Arc::new(registry))
    }

    pub async fn register(&self, addr: String) -> anyhow::Result<()> {
        self.0.register(addr).await
    }

    pub async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
        self.0.unassign_channel(channel_id).await
    }
}

impl ChannelEndNotify for Registry {
    fn notify(&self, channel_id: Uuid) {
        let slf = self.0.clone();
        tokio::spawn(
            async move {
                // deregister channel from voice server
                match slf.unassign_channel(channel_id).await {
                    Ok(_) => {
                        tracing::info!("dropped channel {}", channel_id);
                    }
                    Err(e) => {
                        tracing::warn!("failed to unassign self from channel {}", e);
                    }
                }
            }
            .instrument(tracing::info_span!("unassign_server", channel_id=%channel_id)),
        );
    }
}
