use async_trait::async_trait;
use uuid::Uuid;
use voice_server::{VoiceServer, VoiceServerImpl};
use voices_channels::grpc::proto::channels_server::Channels;
use voices_channels::grpc::service::ChannelsImpl;

#[async_trait]
pub trait ChannelRegistry: Clone + Send + Sync + 'static {
    type Voice: VoiceServer + Clone;
    type RegistryHandle: Channels;
    async fn get_voice(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>>;

    fn registry_handle(&self) -> &Self::RegistryHandle;
}

/// Integrated Channel Registry
///
/// Owns a both [`ChannelsImpl`] and [`VoiceServerImpl`]
#[derive(Clone)]
pub struct LocalChannelRegistry {
    channels: ChannelsImpl,
    voice: VoiceServerImpl,
}

impl LocalChannelRegistry {
    pub fn new(channels: ChannelsImpl, voice: VoiceServerImpl) -> Self {
        Self { channels, voice }
    }
}

#[async_trait]
impl ChannelRegistry for LocalChannelRegistry {
    type Voice = VoiceServerImpl;
    type RegistryHandle = ChannelsImpl;

    async fn get_voice(
        &self,
        channel_id: Uuid,
        _reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>> {
        let request = voices_channels::grpc::proto::GetChannelRequest {
            channel_id: channel_id.to_string(),
        };
        self.channels
            .get_channel(tonic::Request::new(request))
            .await?;
        let request = voice_server::grpc::proto::AssignChannelRequest {
            channel_id: channel_id.to_string(),
        };
        self.voice
            .assign_channel(tonic::Request::new(request))
            .await?;
        Ok(Some(self.voice.clone()))
    }

    fn registry_handle(&self) -> &Self::RegistryHandle {
        &self.channels
    }
}

#[derive(Clone)]
pub struct DistributedChannelRegistry {
    channels:
        voices_channels::grpc::proto::channels_client::ChannelsClient<tonic::transport::Channel>,
}

impl DistributedChannelRegistry {
    pub fn new(endpoint: tonic::transport::Uri) -> Self {
        tracing::info!(endpoint=%endpoint, "setting up remote channel registry client");
        let endpoint = tonic::transport::Endpoint::from(endpoint);
        let ch = endpoint.connect_lazy();
        Self {
            channels: voices_channels::grpc::proto::channels_client::ChannelsClient::new(ch),
        }
    }
}

#[async_trait]
impl ChannelRegistry for DistributedChannelRegistry {
    type Voice = voice_server::grpc::proto::voice_server_client::VoiceServerClient<
        tonic::transport::Channel,
    >;
    type RegistryHandle =
        voices_channels::grpc::proto::channels_client::ChannelsClient<tonic::transport::Channel>;

    async fn get_voice(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>> {
        let request = voices_channels::grpc::proto::GetChannelRequest {
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
                let client =
                    voice_server::grpc::proto::voice_server_client::VoiceServerClient::connect(
                        host,
                    )
                    .await?;
                let msg = voice_server::grpc::proto::StatusRequest {
                    channel_id: channel_id.to_string(),
                };
                if let Err(e) = client.status(tonic::Request::new(msg)).await {
                    if e.code() == tonic::Code::NotFound {
                        tracing::info!("voice server lost the channel");
                        let request = voice_server::grpc::proto::AssignChannelRequest {
                            channel_id: channel_id.to_string(),
                        };
                        client.assign_channel(tonic::Request::new(request)).await?;
                    } else {
                        return Err(e.into());
                    }
                }
                return Ok(Some(client));
            }
            Some(_) | None => {
                tracing::info!(reassign, ?channel_id, "requesting assignment of channel");
                let request = voices_channels::grpc::proto::AssignChannelRequest {
                    channel_id: channel_id.to_string(),
                    reassign,
                };
                let resp = self
                    .channels
                    .clone()
                    .assign_channel(tonic::Request::new(request))
                    .await?
                    .into_inner();
                let client =
                    voice_server::grpc::proto::voice_server_client::VoiceServerClient::connect(
                        resp.voice_server_host,
                    )
                    .await?;
                let request = voice_server::grpc::proto::AssignChannelRequest {
                    channel_id: channel_id.to_string(),
                };
                client.assign_channel(tonic::Request::new(request)).await?;
                return Ok(Some(client));
            }
        }
    }
    fn registry_handle(&self) -> &Self::RegistryHandle {
        &self.channels
    }
}
