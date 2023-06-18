use anyhow::Context;
use async_trait::async_trait;
use uuid::Uuid;

use super::voice_channels_proto::channels_client::ChannelsClient;
use super::voice_channels_proto::{self, AssignChannelRequest, GetChannelRequest};
use crate::voice_instance::{RemoteVoiceHost, VoiceHost};

use super::{ChannelRegistry, GetVoiceHost};

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
impl GetVoiceHost for DistributedChannelRegistry {
    type Voice = RemoteVoiceHost;

    async fn get_voice_host_for(
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

#[async_trait]
impl ChannelRegistry for DistributedChannelRegistry {
    async fn create_server(&self, name: String) -> anyhow::Result<Uuid> {
        let response = self
            .channels
            .clone()
            .create_server(tonic::Request::new(
                voice_channels_proto::CreateServerRequest { name },
            ))
            .await?
            .into_inner();
        response.server_id.parse().map_err(|e| {
            tracing::error!("failed to parse server_id '{}': {}", response.server_id, e);
            anyhow::Error::from(e)
        })
    }
    async fn get_server(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::ServerWithChannels>> {
        let response = match self
            .channels
            .clone()
            .get_server(tonic::Request::new(
                voice_channels_proto::GetServerRequest {
                    server_id: id.to_string(),
                },
            ))
            .await
        {
            Ok(response) => response.into_inner(),
            Err(status) => {
                if let tonic::Code::NotFound = status.code() {
                    return Ok(None);
                } else {
                    return Err(status.into());
                }
            }
        };
        response.try_into().map(Some)
    }
    // FIXME: should be some proper error enum instead of result<option<>>
    async fn create_channel(&self, server_id: Uuid, name: String) -> anyhow::Result<Option<Uuid>> {
        let response = match self
            .channels
            .clone()
            .create_channel(tonic::Request::new(
                voice_channels_proto::CreateChannelRequest {
                    server_id: server_id.to_string(),
                    name,
                },
            ))
            .await
        {
            Ok(response) => response.into_inner(),
            Err(status) => {
                if let tonic::Code::NotFound = status.code() {
                    return Ok(None);
                } else {
                    return Err(status.into());
                }
            }
        };
        Ok(Some(response.channel_id.parse()?))
    }

    async fn get_channel(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::Channel>> {
        let response = match self
            .channels
            .clone()
            .get_channel(tonic::Request::new(
                voice_channels_proto::GetChannelRequest {
                    channel_id: id.to_string(),
                },
            ))
            .await
        {
            Ok(response) => response.into_inner(),
            Err(status) => {
                if let tonic::Code::NotFound = status.code() {
                    return Ok(None);
                } else {
                    return Err(status.into());
                }
            }
        };
        response.try_into().map(Some)
    }
    async fn get_servers(
        &self,
        page: Option<i64>,
        per_page: Option<i64>,
    ) -> anyhow::Result<(Vec<voices_channels_models::Server>, i64)> {
        let response = self
            .channels
            .clone()
            .get_servers(tonic::Request::new(
                voice_channels_proto::GetServersRequest { page, per_page },
            ))
            .await?
            .into_inner();
        let voice_channels_proto::GetServersResponse { pages, info } = response;
        info.into_iter()
            .map(|v| {
                Ok(voices_channels_models::Server {
                    id: v.server_id.parse()?,
                    name: v.name,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map(|v| (v, pages))
    }
    async fn cleanup_stale_voice_servers(&self) -> anyhow::Result<Vec<Uuid>> {
        let response = self
            .channels
            .clone()
            .cleanup_stale_voice_servers(tonic::Request::new(
                voice_channels_proto::CleanupStaleVoiceServersRequest {},
            ))
            .await?
            .into_inner();
        Ok(response
            .deleted
            .into_iter()
            .map(|v| v.parse::<Uuid>())
            .collect::<Result<Vec<Uuid>, _>>()?)
    }
}

impl TryFrom<voice_channels_proto::GetServerResponse>
    for voices_channels_models::ServerWithChannels
{
    type Error = anyhow::Error;

    fn try_from(value: voice_channels_proto::GetServerResponse) -> Result<Self, Self::Error> {
        let voice_channels_proto::GetServerResponse {
            server_id,
            name,
            channels,
        } = value;
        Ok(voices_channels_models::ServerWithChannels {
            id: server_id.parse()?,
            name,
            channels: channels
                .into_iter()
                .map(voices_channels_models::ChannelInfo::try_from)
                .collect::<anyhow::Result<Vec<_>>>()?,
        })
    }
}

impl TryFrom<voice_channels_proto::ChannelInfo> for voices_channels_models::ChannelInfo {
    type Error = anyhow::Error;

    fn try_from(value: voice_channels_proto::ChannelInfo) -> Result<Self, Self::Error> {
        let voice_channels_proto::ChannelInfo { channel_id, name } = value;
        Ok(voices_channels_models::ChannelInfo {
            id: channel_id.parse()?,
            name,
        })
    }
}

impl TryFrom<voice_channels_proto::GetChannelResponse> for voices_channels_models::Channel {
    type Error = anyhow::Error;

    fn try_from(value: voice_channels_proto::GetChannelResponse) -> Result<Self, Self::Error> {
        let voice_channels_proto::GetChannelResponse {
            channel_id,
            name,
            voice_server_host,
            server_id,
            updated_at,
        } = value;
        let updated_at = updated_at.context("updated_at is missing")?;
        let updated_at =
            chrono::NaiveDateTime::from_timestamp_opt(updated_at.seconds, updated_at.nanos as _)
                .context("invalid timestamp")?
                .and_utc();
        Ok(voices_channels_models::Channel {
            info: voices_channels_models::ChannelInfo {
                id: channel_id.parse()?,
                name,
            },
            server_id: server_id.parse()?,
            assigned_to: voice_server_host,
            updated_at,
        })
    }
}
