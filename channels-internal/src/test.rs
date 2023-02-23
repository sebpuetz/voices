use anyhow::Context;
use assert_matches::assert_matches;
use chrono::{DateTime, NaiveDateTime, Utc};
use deadpool_diesel::postgres::Pool;
use uuid::Uuid;

use crate::grpc::proto::channels_server::Channels as _;
use crate::grpc::proto::{
    self, AssignChannelRequest, ChannelInfo, CreateChannelRequest, CreateServerRequest,
    GetChannelRequest, GetChannelResponse, GetServerRequest, GetServerResponse,
    RegisterVoiceServerRequest,
};
use crate::grpc::service::ChannelsImpl;
use crate::models::voice_server::VoiceServer;
use crate::test_helper::PoolGuard;
use crate::ChannelsConfig;

#[tokio::test]
async fn test_create_and_get_server() {
    let channels = Channels::new(None).await.unwrap();
    let name = "foo";
    let server_id = create_server(name, &channels).await.unwrap();
    let response = get_server(server_id, &channels).await.unwrap();
    assert_eq!(name, response.name);
    assert_matches!(&*response.channels, &[]);
}

#[tokio::test]
async fn test_get_server_not_found() {
    let channels = Channels::new(None).await.unwrap();
    let get_request = GetServerRequest {
        server_id: Uuid::new_v4().to_string(),
    };
    let error = channels
        .get_server(tonic::Request::new(get_request))
        .await
        .unwrap_err();
    assert_matches!(error.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_create_channel_happy() {
    let channels = Channels::new(None).await.unwrap();
    let name = "foo";
    let server_id = create_server(name, &channels).await.unwrap();
    let channel_id = create_channel(server_id.clone(), name, &channels)
        .await
        .unwrap();
    let channel = get_channel(channel_id, &channels).await.unwrap();
    assert_eq!(name, channel.name);
    assert_matches!(channel.voice_server_host, None);

    let server = get_server(server_id, &channels).await.unwrap();
    assert_eq!(
        server.channels,
        vec![ChannelInfo {
            channel_id: channel.channel_id,
            name: channel.name
        }]
    );
}

#[tokio::test]
async fn test_register_voice_server() {
    let channels = Channels::new(None).await.unwrap();
    let vs_id = Uuid::new_v4();
    let addr = "http://lolhost";
    register_voice_server(&vs_id.to_string(), addr, &channels)
        .await
        .unwrap();
    let pool = channels.db();
    let vs = VoiceServer::get_active(vs_id, chrono::Duration::seconds(10), pool)
        .await
        .unwrap();
    let registered_at = assert_matches!(vs, Some(VoiceServer { id, host_url, last_seen }) => {
        assert_eq!(vs_id, id);
        assert_eq!(addr, host_url);
        last_seen
    });
    let next_refresh_at = register_voice_server(&vs_id.to_string(), addr, &channels)
        .await
        .unwrap();
    let vs = VoiceServer::get_active(vs_id, chrono::Duration::seconds(10), pool)
        .await
        .unwrap();
    let refreshed_at = assert_matches!(vs, Some(VoiceServer { id, host_url, last_seen }) => {
        assert_eq!(vs_id, id);
        assert_eq!(addr, host_url);
        last_seen
    });
    assert_eq!(
        next_refresh_at
            - channels
                .channels_impl
                .config()
                .voice_server_staleness_duration(),
        refreshed_at
    );
    assert!(refreshed_at > registered_at);
}

#[tokio::test]
async fn test_create_channel_and_assign_server() {
    let channels = Channels::new(None).await.unwrap();
    let name = "foo";
    let server_id = create_server(name, &channels).await.unwrap();
    let channel_id = create_channel(server_id.clone(), name, &channels)
        .await
        .unwrap();
    let vs_id = Uuid::new_v4();
    let addr = "http://lolhost";
    register_voice_server(&vs_id.to_string(), addr, &channels)
        .await
        .unwrap();
    let host_response = assign_channel(&channel_id, false, &channels).await.unwrap();

    let channel = get_channel(channel_id, &channels).await;
    assert_matches!(channel, Ok(GetChannelResponse {
        channel_id: _,
        name: channel_name,
        voice_server_host: Some(host),
    }) => {
        assert_eq!(channel_name, name);
        assert_eq!(addr, host);
        assert_eq!(host_response, host);
    });
}

async fn register_voice_server(
    id: &str,
    addr: &str,
    channels: &Channels,
) -> anyhow::Result<DateTime<Utc>> {
    let request = RegisterVoiceServerRequest {
        id: id.to_string(),
        addr: addr.to_string(),
    };
    let request = tonic::Request::new(request);
    let response = channels.register_voice_server(request).await?.into_inner();
    let valid_until = response.valid_until.context("missing valid until")?;
    let valid_until = DateTime::<Utc>::from_utc(
        NaiveDateTime::from_timestamp_opt(valid_until.seconds, valid_until.nanos as _).unwrap(),
        Utc,
    );
    Ok(valid_until)
}

async fn assign_channel(
    channel_id: &str,
    reassign: bool,
    channels: &Channels,
) -> anyhow::Result<String> {
    let request = AssignChannelRequest {
        reassign,
        channel_id: channel_id.to_string(),
    };
    let request = tonic::Request::new(request);
    let response = channels.assign_channel(request).await?.into_inner();
    Ok(response.voice_server_host)
}

async fn create_server(name: &str, channels: &Channels) -> anyhow::Result<String> {
    let request = CreateServerRequest {
        name: name.to_string(),
    };
    let request = tonic::Request::new(request);
    Ok(channels
        .create_server(request)
        .await?
        .into_inner()
        .server_id)
}

async fn get_server(server_id: String, channels: &Channels) -> anyhow::Result<GetServerResponse> {
    let get_request = GetServerRequest { server_id };
    Ok(channels
        .get_server(tonic::Request::new(get_request))
        .await?
        .into_inner())
}

async fn create_channel(
    server_id: String,
    name: &str,
    channels: &Channels,
) -> anyhow::Result<String> {
    let request = CreateChannelRequest {
        server_id,
        name: name.to_string(),
    };
    Ok(channels
        .create_channel(tonic::Request::new(request))
        .await?
        .into_inner()
        .channel_id)
}

async fn get_channel(
    channel_id: String,
    channels: &Channels,
) -> anyhow::Result<GetChannelResponse> {
    let get_request = GetChannelRequest { channel_id };
    Ok(channels
        .get_channel(tonic::Request::new(get_request))
        .await?
        .into_inner())
}

pub async fn new_channels_db() -> anyhow::Result<PoolGuard> {
    let _ = dotenvy::dotenv();
    let base_db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432".into());
    PoolGuard::setup_default(base_db_url).await
}

pub struct Channels {
    channels_impl: ChannelsImpl,
    _db_guard: Option<PoolGuard>,
}

impl Channels {
    pub async fn new(db_url: Option<String>) -> anyhow::Result<Self> {
        let (db_guard, database_url) = match db_url {
            Some(db) => (None, db),
            None => {
                let guard = new_channels_db().await?;
                let full_url = guard.full_database_url();
                (Some(guard), full_url)
            }
        };
        let cfg = ChannelsConfig {
            migrate: false,
            database_url: database_url.into(),
        };
        Ok(Channels {
            channels_impl: cfg.server().await?,
            _db_guard: db_guard,
        })
    }

    pub fn db(&self) -> &Pool {
        self.channels_impl.db()
    }
}

#[async_trait::async_trait]
impl proto::channels_server::Channels for Channels {
    async fn cleanup_stale_voice_servers(
        &self,
        request: tonic::Request<proto::CleanupStaleVoiceServersRequest>,
    ) -> Result<tonic::Response<proto::CleanupStaleVoiceServersResponse>, tonic::Status> {
        self.channels_impl
            .cleanup_stale_voice_servers(request)
            .await
    }

    async fn register_voice_server(
        &self,
        request: tonic::Request<proto::RegisterVoiceServerRequest>,
    ) -> Result<tonic::Response<proto::RegisterVoiceServerResponse>, tonic::Status> {
        self.channels_impl.register_voice_server(request).await
    }

    async fn get_servers(
        &self,
        request: tonic::Request<proto::GetServersRequest>,
    ) -> Result<tonic::Response<proto::GetServersResponse>, tonic::Status> {
        self.channels_impl.get_servers(request).await
    }

    async fn create_server(
        &self,
        request: tonic::Request<proto::CreateServerRequest>,
    ) -> Result<tonic::Response<proto::CreateServerResponse>, tonic::Status> {
        self.channels_impl.create_server(request).await
    }

    async fn get_server(
        &self,
        request: tonic::Request<proto::GetServerRequest>,
    ) -> Result<tonic::Response<proto::GetServerResponse>, tonic::Status> {
        self.channels_impl.get_server(request).await
    }

    async fn create_channel(
        &self,
        request: tonic::Request<proto::CreateChannelRequest>,
    ) -> Result<tonic::Response<proto::CreateChannelResponse>, tonic::Status> {
        self.channels_impl.create_channel(request).await
    }

    async fn get_channel(
        &self,
        request: tonic::Request<proto::GetChannelRequest>,
    ) -> Result<tonic::Response<proto::GetChannelResponse>, tonic::Status> {
        self.channels_impl.get_channel(request).await
    }

    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        self.channels_impl.assign_channel(request).await
    }
    async fn unassign_channel(
        &self,
        request: tonic::Request<proto::UnassignChannelRequest>,
    ) -> Result<tonic::Response<proto::UnassignChannelResponse>, tonic::Status> {
        self.channels_impl.unassign_channel(request).await
    }
}
