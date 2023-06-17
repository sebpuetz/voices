use async_trait::async_trait;
use uuid::Uuid;
use voices_channels::grpc::proto::channels_server::Channels;
use voices_channels::grpc::service::ChannelsImpl;

use crate::voice_instance::IntegratedVoiceHost;

use super::ChannelRegistry;

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
impl ChannelRegistry for LocalChannelRegistry {
    type Voice = IntegratedVoiceHost;

    async fn get_voice_host(
        &self,
        channel_id: Uuid,
        _reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>> {
        let request = voices_channels::grpc::proto::GetChannelRequest {
            channel_id: channel_id.to_string(),
        };
        // ensure channel exists
        self.channels
            .get_channel(tonic::Request::new(request))
            .await?;
        // assign channel to the local voice server
        self.voice.assign_channel(channel_id).await?;
        Ok(Some(self.voice.clone()))
    }
}
