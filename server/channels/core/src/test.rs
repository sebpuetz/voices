use assert_matches::assert_matches;
use deadpool_diesel::postgres::Pool;
use uuid::Uuid;

use crate::db_models::voice_server::VoiceServerDb;

use crate::test_helper::PoolGuard;
use crate::{ChannelsConfig, ChannelsImpl};

#[tokio::test]
async fn test_create_and_get_server() {
    let channels = Channels::new(None).await.unwrap();
    let name = "foo";
    let server = channels.create_server(name.into()).await.unwrap();
    let response = channels.get_server(server).await.unwrap();

    assert_matches!(
        response,
        Some(crate::models::ServerWithChannels { id: _, name: res_name, channels }) => {
            assert_eq!(name, res_name);
            assert_eq!(channels, Vec::new());
        }
    );
}

#[tokio::test]
async fn test_get_server_not_found() {
    let channels = Channels::new(None).await.unwrap();
    let res = channels.get_server(Uuid::new_v4()).await;
    assert_matches!(res, Ok(None));
}

#[tokio::test]
async fn test_create_channel_happy() {
    let channels = Channels::new(None).await.unwrap();
    let name = "foo";

    let server_id = channels.create_server(name.into()).await.unwrap();
    let channel_id = channels
        .create_channel(server_id, name.into())
        .await
        .unwrap();
    let channel = channels.get_channel(channel_id).await;
    assert_matches!(channel, Ok(Some(channel)) => {
        assert_eq!(name, channel.info.name);
        assert_matches!(channel.assigned_to, None);
    } );

    let server = channels.get_server(server_id).await;

    assert_matches!(server, Ok(Some(server)) => {
        assert_eq!(
            server.channels,
            vec![crate::models::ChannelInfo {
                id: channel_id,
                name: name.into()
            }]
        )
    });
}

#[tokio::test]
async fn test_register_voice_server() {
    let channels = Channels::new(None).await.unwrap();
    let vs_id = Uuid::new_v4();
    let addr = "http://lolhost/";

    channels
        .register_voice_server(vs_id, addr.parse().unwrap())
        .await
        .unwrap();
    let pool = channels.db();
    let vs = VoiceServerDb::get_active(vs_id, chrono::Duration::seconds(10), pool)
        .await
        .unwrap();
    let registered_at = assert_matches!(vs, Some(VoiceServerDb { id, host_url, last_seen }) => {
        assert_eq!(vs_id, id);
        assert_eq!(addr, host_url);
        last_seen
    });
    let next_refresh_at = channels
        .register_voice_server(vs_id, addr.parse().unwrap())
        .await
        .unwrap();
    let vs = VoiceServerDb::get_active(vs_id, chrono::Duration::seconds(10), pool)
        .await
        .unwrap();
    let refreshed_at = assert_matches!(vs, Some(VoiceServerDb { id, host_url, last_seen }) => {
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
    let server = channels.create_server(name.into()).await.unwrap();
    let channel = channels.create_channel(server, name.into()).await.unwrap();

    let vs_id = Uuid::new_v4();
    let addr = "http://lolhost/";
    channels
        .register_voice_server(vs_id, addr.parse().unwrap())
        .await
        .unwrap();

    let host_response = channels.assign_channel(channel, false).await.unwrap();

    let channel = channels.get_channel(channel).await;
    assert_matches!(channel, Ok(Some(crate::models::Channel {
        info: crate::models::ChannelInfo {
            id: _,
            name: channel_name,
        },
        assigned_to: Some(host),
        ..
    })) => {
        assert_eq!(channel_name, name);
        assert_eq!(addr, host);
        assert_eq!(host_response.host_url, host);
    });
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

impl std::ops::Deref for Channels {
    type Target = ChannelsImpl;

    fn deref(&self) -> &Self::Target {
        &self.channels_impl
    }
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
