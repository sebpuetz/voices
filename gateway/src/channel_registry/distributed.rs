use anyhow::Context;
use async_trait::async_trait;
use uuid::Uuid;

use super::voice_channels_proto::channels_client::ChannelsClient;
use super::voice_channels_proto::{AssignChannelRequest, GetChannelRequest};
use crate::voice_instance::{RemoteVoiceHost, VoiceHost};

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
