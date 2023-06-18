use std::str::FromStr;

use chrono::SubsecRound;
use proto::channels_server::*;
use tonic::{async_trait, Status};

use voices_channels::{ChannelsConfig, ChannelsImpl};

use super::proto;

pub struct ChannelsGrpc(ChannelsImpl);

impl ChannelsGrpc {
    pub async fn new(
        cfg: &ChannelsConfig,
    ) -> anyhow::Result<proto::channels_server::ChannelsServer<ChannelsGrpc>> {
        let server_impl = cfg.server().await?;
        Ok(proto::channels_server::ChannelsServer::new(Self(
            server_impl,
        )))
    }
}

#[async_trait]
impl Channels for ChannelsGrpc {
    async fn cleanup_stale_voice_servers(
        &self,
        _: tonic::Request<proto::CleanupStaleVoiceServersRequest>,
    ) -> Result<tonic::Response<proto::CleanupStaleVoiceServersResponse>, tonic::Status> {
        let deleted = self
            .0
            .cleanup_stale_voice_servers()
            .await
            .map_err(error_mapper("cleanup_stale_voice_servers failed"))?;
        Ok(tonic::Response::new(
            proto::CleanupStaleVoiceServersResponse {
                deleted: deleted.iter().map(ToString::to_string).collect(),
            },
        ))
    }

    async fn register_voice_server(
        &self,
        request: tonic::Request<proto::RegisterVoiceServerRequest>,
    ) -> Result<tonic::Response<proto::RegisterVoiceServerResponse>, tonic::Status> {
        let req = request.into_inner();
        let id = parse(&req.id, "voice_server_id")?;
        let url = parse(&req.addr, "addr")?;
        let valid_until = self
            .0
            .register_voice_server(id, url)
            .await
            .map_err(error_mapper("register_voice_server failed"))?;
        let seconds = valid_until.trunc_subsecs(0).timestamp();
        let nanos = valid_until.timestamp_subsec_nanos();
        let valid_until = prost_types::Timestamp {
            nanos: nanos as _,
            seconds,
        };
        Ok(tonic::Response::new(proto::RegisterVoiceServerResponse {
            valid_until: Some(valid_until),
        }))
    }

    async fn get_servers(
        &self,
        request: tonic::Request<proto::GetServersRequest>,
    ) -> Result<tonic::Response<proto::GetServersResponse>, tonic::Status> {
        let req = request.into_inner();
        let (servers, pages) = self
            .0
            .get_servers(req.page.unwrap_or(1), req.per_page.unwrap_or(10).min(100))
            .await
            .map_err(error_mapper("get_servers failed"))?;
        Ok(tonic::Response::new(proto::GetServersResponse {
            info: servers.into_iter().map(Into::into).collect(),
            pages,
        }))
    }

    async fn get_server(
        &self,
        request: tonic::Request<proto::GetServerRequest>,
    ) -> Result<tonic::Response<proto::GetServerResponse>, tonic::Status> {
        let req = request.into_inner();
        let sid = parse(&req.server_id, "server_id")?;
        let server = self
            .0
            .get_server(sid)
            .await
            .map_err(error_mapper("get_server failed"))?
            .ok_or_else(|| tonic::Status::not_found("server not found"))?;

        Ok(tonic::Response::new(server.into()))
    }

    async fn create_server(
        &self,
        request: tonic::Request<proto::CreateServerRequest>,
    ) -> Result<tonic::Response<proto::CreateServerResponse>, tonic::Status> {
        let req = request.into_inner();
        let sid = self
            .0
            .create_server(req.name)
            .await
            .map_err(error_mapper("create_server failed"))?;
        Ok(tonic::Response::new(proto::CreateServerResponse {
            server_id: sid.to_string(),
        }))
    }

    async fn create_channel(
        &self,
        request: tonic::Request<proto::CreateChannelRequest>,
    ) -> Result<tonic::Response<proto::CreateChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let sid = parse(&req.server_id, "server_id")?;
        let chan_id = self
            .0
            .create_channel(sid, req.name)
            .await
            .map_err(error_mapper("create_channel failed"))?;
        Ok(tonic::Response::new(proto::CreateChannelResponse {
            channel_id: chan_id.to_string(),
        }))
    }

    #[tracing::instrument(skip_all, fields(channel_id=%request.get_ref().channel_id))]
    async fn get_channel(
        &self,
        request: tonic::Request<proto::GetChannelRequest>,
    ) -> Result<tonic::Response<proto::GetChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let id = parse(&req.channel_id, "channel_id")?;
        let channel = self
            .0
            .get_channel(id)
            .await
            .map_err(error_mapper("get_channel failed"))?
            .ok_or_else(|| tonic::Status::not_found("channel does not exist"))?;

        Ok(tonic::Response::new(channel.into()))
    }

    #[tracing::instrument(skip_all, fields(channel_id=%request.get_ref().channel_id))]
    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let channel_id = parse(&req.channel_id, "channel_id")?;
        let srv = self
            .0
            .assign_channel(channel_id, req.reassign)
            .await
            .map_err(error_mapper("assign_channel failed"))?;
        Ok(tonic::Response::new(proto::AssignChannelResponse {
            voice_server_host: srv.host_url,
        }))
    }

    async fn unassign_channel(
        &self,
        request: tonic::Request<proto::UnassignChannelRequest>,
    ) -> Result<tonic::Response<proto::UnassignChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let channel_id = parse(&req.channel_id, "channel_id")?;
        let sid = parse(&req.server_id, "server_id")?;
        self.0
            .unassign_channel(sid, channel_id)
            .await
            .map_err(error_mapper("unassign_channel failed"))?;

        tracing::info!("unassigned channel {:?}", channel_id);
        Ok(tonic::Response::new(proto::UnassignChannelResponse {}))
    }
}

fn parse<S, E>(value: &str, field_name: &'static str) -> Result<S, Status>
where
    S: FromStr<Err = E>,
    E: std::fmt::Debug,
{
    value.parse().map_err(|e| {
        tracing::warn!(error=?e, "bad {}", field_name);
        Status::invalid_argument(format!("bad {}", field_name))
    })
}

impl From<voices_channels_models::Server> for proto::get_servers_response::ServerInfo {
    fn from(value: voices_channels_models::Server) -> Self {
        proto::get_servers_response::ServerInfo {
            server_id: value.id.to_string(),
            name: value.name,
        }
    }
}

impl From<voices_channels_models::Channel> for proto::GetChannelResponse {
    fn from(value: voices_channels_models::Channel) -> Self {
        let seconds = value.updated_at.trunc_subsecs(0).timestamp();
        let nanos = value.updated_at.timestamp_subsec_nanos();
        proto::GetChannelResponse {
            name: value.info.name,
            channel_id: value.info.id.to_string(),
            voice_server_host: value.assigned_to,
            updated_at: Some(prost_types::Timestamp {
                seconds,
                nanos: nanos as _,
            }),
            server_id: value.server_id.to_string(),
        }
    }
}

impl From<voices_channels_models::ServerWithChannels> for proto::GetServerResponse {
    fn from(value: voices_channels_models::ServerWithChannels) -> Self {
        proto::GetServerResponse {
            server_id: value.id.to_string(),
            name: value.name,
            channels: value.channels.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<voices_channels_models::ChannelInfo> for proto::ChannelInfo {
    fn from(value: voices_channels_models::ChannelInfo) -> Self {
        proto::ChannelInfo {
            channel_id: value.id.to_string(),
            name: value.name,
        }
    }
}

fn error_mapper(msg: &'static str) -> impl FnOnce(anyhow::Error) -> tonic::Status {
    move |error: anyhow::Error| {
        tracing::warn!("{}: {}", msg, error);
        tonic::Status::internal("internal server error")
    }
}
