use async_trait::async_trait;
use uuid::Uuid;
use voices_channels::ChannelsImpl;

use crate::voice_instance::IntegratedVoiceHost;

use super::{ChannelRegistry, GetVoiceHost};

/// Integrated Channel Registry
///
/// Owns a both [`ChannelsImpl`] and [`IntegratedVoiceHost`]
#[derive(Clone)]
pub struct LocalChannelRegistry {
    channels: ChannelsImpl,
    voice: IntegratedVoiceHost,
}

impl LocalChannelRegistry {
    pub fn new(channels: ChannelsImpl, voice: IntegratedVoiceHost) -> Self {
        Self { channels, voice }
    }
}

#[async_trait]
impl GetVoiceHost for LocalChannelRegistry {
    type Voice = IntegratedVoiceHost;

    async fn get_voice_host_for(
        &self,
        channel_id: Uuid,
        _reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>> {
        // ensure channel exists
        if self.channels.get_channel(channel_id).await?.is_none() {
            return Ok(None);
        }

        // assign channel to the local voice server
        self.voice.assign_channel(channel_id).await?;
        Ok(Some(self.voice.clone()))
    }
}

#[async_trait]
impl ChannelRegistry for LocalChannelRegistry {
    async fn create_server(&self, name: String) -> anyhow::Result<Uuid> {
        self.channels.create_server(name).await
    }

    async fn get_server(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::ServerWithChannels>> {
        self.channels.get_server(id).await
    }
    // FIXME: should be some proper error enum instead of result<option<>>
    async fn create_channel(&self, server_id: Uuid, name: String) -> anyhow::Result<Option<Uuid>> {
        // FIXME: ChannelsImpl doesn't signal when creation fails because of missing server
        self.channels
            .create_channel(server_id, name)
            .await
            .map(Some)
    }
    async fn get_channel(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::Channel>> {
        self.channels.get_channel(id).await
    }
    async fn get_servers(
        &self,
        page: Option<i64>,
        per_page: Option<i64>,
    ) -> anyhow::Result<(Vec<voices_channels_models::Server>, i64)> {
        self.get_servers(page, per_page).await
    }
    async fn cleanup_stale_voice_servers(&self) -> anyhow::Result<Vec<Uuid>> {
        self.cleanup_stale_voice_servers().await
    }
}
