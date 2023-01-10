use std::str::FromStr;

use deadpool::Runtime;
use deadpool_diesel::postgres::{Manager, Pool};
use proto::channels_server::*;
use tonic::{async_trait, Status};

use crate::db::{DbError, Paginate};
use crate::models::channel::{Channel, NewChannel};
use crate::models::server::{NewServer, Server};
use crate::models::voice_server::{NewVoiceServer, VoiceServer};

use super::proto;

#[derive(Clone)]
pub struct ChannelsImpl {
    db: Pool,
}

impl ChannelsImpl {
    pub async fn new_from_pg_str(conn: String) -> anyhow::Result<Self> {
        let manager = Manager::new(conn, Runtime::Tokio1);
        let db = Pool::builder(manager).build()?;
        Ok(Self { db })
    }

    pub fn new(db: Pool) -> Self {
        Self { db }
    }

    pub fn grpc(self) -> ChannelsServer<Self> {
        ChannelsServer::new(self)
    }
}

#[async_trait]
impl Channels for ChannelsImpl {
    async fn cleanup_stale_voice_servers(
        &self,
        _: tonic::Request<proto::CleanupStaleVoiceServersRequest>,
    ) -> Result<tonic::Response<proto::CleanupStaleVoiceServersResponse>, tonic::Status> {
        let deleted = VoiceServer::cleanup_stale(&self.db).await?;
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
        NewVoiceServer::new(id, req.addr)
            .create_or_update(&self.db)
            .await?;
        Ok(tonic::Response::new(proto::RegisterVoiceServerResponse {}))
    }

    async fn get_servers(
        &self,
        request: tonic::Request<proto::GetServersRequest>,
    ) -> Result<tonic::Response<proto::GetServersResponse>, tonic::Status> {
        let req = request.into_inner();
        let (servers, pages) = Server::all()
            .paginate(req.page.unwrap_or(1))
            .per_page(req.per_page.unwrap_or(10).max(100))
            .load_and_count_pages::<Server>(&self.db)
            .await?;
        Ok(tonic::Response::new(proto::GetServersResponse {
            info: servers
                .into_iter()
                .map(|v| proto::get_servers_response::ServerInfo {
                    server_id: v.id.to_string(),
                    name: v.name,
                })
                .collect(),
            pages,
        }))
    }

    async fn get_server(
        &self,
        request: tonic::Request<proto::GetServerRequest>,
    ) -> Result<tonic::Response<proto::GetServerResponse>, tonic::Status> {
        let req = request.into_inner();
        let sid = parse(&req.server_id, "server_id")?;

        let srv = Server::get(sid, &self.db)
            .await?
            .ok_or_else(|| tonic::Status::not_found("server not found"))?;

        let channels = srv.get_channels(&self.db).await?;
        Ok(tonic::Response::new(proto::GetServerResponse {
            server_id: srv.id.to_string(),
            name: srv.name,
            channels: channels
                .into_iter()
                .map(|ch| proto::ChannelInfo {
                    channel_id: ch.id.to_string(),
                    name: ch.name,
                })
                .collect(),
        }))
    }

    async fn create_server(
        &self,
        request: tonic::Request<proto::CreateServerRequest>,
    ) -> Result<tonic::Response<proto::CreateServerResponse>, tonic::Status> {
        let req = request.into_inner();
        let sid = NewServer::new(req.name).create(&self.db).await?;
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
        let chan_id = NewChannel::new(sid, req.name).create(&self.db).await?;
        Ok(tonic::Response::new(proto::CreateChannelResponse {
            channel_id: chan_id.to_string(),
        }))
    }

    async fn get_channel(
        &self,
        request: tonic::Request<proto::GetChannelRequest>,
    ) -> Result<tonic::Response<proto::GetChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let id = parse(&req.channel_id, "channel_id")?;
        let c = Channel::get(id, &self.db)
            .await?
            .ok_or_else(|| tonic::Status::not_found("channel not found"))?;
        let v = match c.assigned_to {
            Some(id) => VoiceServer::get_active(id, &self.db)
                .await?
                .map(|v| v.host_url),
            None => None,
        };
        Ok(tonic::Response::new(proto::GetChannelResponse {
            channel_id: c.id.to_string(),
            voice_server_host: v,
            name: c.name,
        }))
    }

    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let id = parse(&req.channel_id, "channel_id")?;
        let (srv, load) = VoiceServer::get_smallest_load(&self.db)
            .await?
            .ok_or_else(|| {
                tracing::warn!("no voice servers registered");
                tonic::Status::internal("no voice servers registered")
            })?;
        tracing::info!(
            "assigning channel {} to voice server {:?} with load={}",
            id,
            srv,
            load
        );
        Channel::assign_to(id, srv.id, req.reassign, &self.db)
            .await?
            .ok_or_else(|| tonic::Status::not_found("channel not found"))?;
        Ok(tonic::Response::new(proto::AssignChannelResponse {
            voice_server_host: srv.host_url,
        }))
    }

    async fn unassign_channel(
        &self,
        request: tonic::Request<proto::UnassignChannelRequest>,
    ) -> Result<tonic::Response<proto::UnassignChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let id = parse(&req.channel_id, "channel_id")?;
        Channel::unassign(id, &self.db).await?;
        tracing::info!("unassigned channel {:?}", id);
        Ok(tonic::Response::new(proto::UnassignChannelResponse {}))
    }
}

impl From<DbError> for tonic::Status {
    fn from(value: DbError) -> Self {
        match value {
            DbError::PoolError(_) | DbError::InteractError(_) | DbError::DieselError(_) => {
                tracing::warn!("error: {}", value);
                tonic::Status::internal(value.to_string())
            }
        }
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
