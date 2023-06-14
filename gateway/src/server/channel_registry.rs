use async_trait::async_trait;
use uuid::Uuid;

use voices_channels::grpc::proto::channels_server::Channels;
use voices_channels::grpc::service::ChannelsImpl;

#[async_trait]
pub trait ChannelRegistry: Clone + Send + Sync + 'static {
    type Voice: VoiceHost + Clone;
    async fn get_voice_host(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>>;
}

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

pub use distributed::DistributedChannelRegistry;

use super::voice_instance::{IntegratedVoiceHost, VoiceHost};

mod distributed {
    use anyhow::Context;
    use async_trait::async_trait;
    use uuid::Uuid;

    // use crate::server::channels::voice_channels::VoiceChannels;
    use crate::server::channels::voice_channels_proto::channels_client::ChannelsClient;
    use crate::server::channels::voice_channels_proto::{AssignChannelRequest, GetChannelRequest};
    use crate::server::voice_instance::{RemoteVoiceHost, VoiceHost};

    use super::ChannelRegistry;

    #[derive(Clone)]
    pub struct DistributedChannelRegistry {
        channels: ChannelsClient<tonic::transport::Channel>,
    }

    impl DistributedChannelRegistry {
        pub fn new(endpoint: tonic::transport::Uri) -> Self {
            tracing::info!(endpoint=%endpoint, "setting up remote channel registry client");
            let endpoint = tonic::transport::Endpoint::from(endpoint);
            let ch = endpoint.connect_lazy();
            Self {
                channels: ChannelsClient::new(ch),
            }
        }
    }

    #[async_trait]
    impl ChannelRegistry for DistributedChannelRegistry {
        // TODO: Implement trait VoiceHost for the client and IntegratedVoiceHost
        type Voice = RemoteVoiceHost;

        async fn get_voice_host(
            &self,
            channel_id: Uuid,
            reassign: bool,
        ) -> anyhow::Result<Option<Self::Voice>> {
            let request = GetChannelRequest {
                channel_id: channel_id.to_string(),
            };
            let req = self
                .channels
                .clone()
                .get_channel(tonic::Request::new(request))
                .await?
                .into_inner();
            match req.voice_server_host {
                Some(host) if !reassign => {
                    let client = RemoteVoiceHost::new(
                        host.parse()
                            .context("failed to parse voice server host uri")?,
                    );
                    if let Err(e) = client.status(channel_id).await {
                        if e.code() == tonic::Code::NotFound {
                            tracing::info!("voice server lost the channel");
                            client.assign_channel(channel_id).await?;
                        } else {
                            return Err(e.into());
                        }
                    }
                    return Ok(Some(client));
                }
                Some(_) | None => {
                    tracing::info!(reassign, ?channel_id, "requesting assignment of channel");
                    let request = AssignChannelRequest {
                        channel_id: channel_id.to_string(),
                        reassign,
                    };
                    let resp = self
                        .channels
                        .clone()
                        .assign_channel(tonic::Request::new(request))
                        .await?
                        .into_inner();
                    let client = RemoteVoiceHost::new(
                        resp.voice_server_host
                            .parse()
                            .context("failed to parse voice server host uri")?,
                    );
                    client.assign_channel(channel_id).await?;

                    return Ok(Some(client));
                }
            }
        }
    }
}
