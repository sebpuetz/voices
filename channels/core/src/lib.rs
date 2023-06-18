use anyhow::Context;
use clap::Parser;
use deadpool::Runtime;
use deadpool_diesel::postgres::{Manager, Pool};
use secrecy::ExposeSecret;
use voices_channels_models::{Server, ServerWithChannels, VoiceServer};

pub mod db;
pub mod db_models;
pub mod error;
pub mod schema;

mod models;
#[cfg(test)]
mod test;
#[cfg(any(test, feature = "test"))]
pub mod test_helper;

use chrono::{DateTime, Utc};
use url::Url;
use uuid::Uuid;

use crate::db::Paginate;
use crate::db_models::channel::{ChannelDb, NewChannel};
use crate::db_models::server::{NewServer, ServerDb};
use crate::db_models::voice_server::{NewVoiceServer, VoiceServerDb};

#[derive(Clone)]
pub struct ChannelsImpl {
    db: Pool,
    config: Config,
}

#[derive(Clone)]
pub struct Config {
    voice_server_staleness_duration: chrono::Duration,
}

impl Config {
    pub fn voice_server_staleness_duration(&self) -> chrono::Duration {
        self.voice_server_staleness_duration
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_server_staleness_duration: chrono::Duration::seconds(35),
        }
    }
}

impl ChannelsImpl {
    pub async fn new_from_pg_str(conn: String) -> anyhow::Result<Self> {
        let manager = Manager::new(conn, Runtime::Tokio1);
        let db = Pool::builder(manager).build()?;
        Ok(Self {
            db,
            config: Default::default(),
        })
    }

    pub fn new(db: Pool) -> Self {
        Self {
            db,
            config: Default::default(),
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn cleanup_stale_voice_servers(&self) -> anyhow::Result<Vec<Uuid>> {
        let deleted =
            VoiceServerDb::cleanup_stale(self.config.voice_server_staleness_duration, &self.db)
                .await?;
        Ok(deleted)
    }

    #[tracing::instrument(skip_all, fields(voice_server_id=%id))]
    pub async fn register_voice_server(
        &self,
        id: Uuid,
        host_url: Url,
    ) -> anyhow::Result<DateTime<Utc>> {
        let (_, refreshed_at) = NewVoiceServer::new(id, host_url.to_string())
            .create_or_update(&self.db)
            .await?;
        Ok(refreshed_at + self.config.voice_server_staleness_duration)
    }

    pub async fn get_servers(
        &self,
        page: i64,
        per_page: i64,
    ) -> anyhow::Result<(Vec<Server>, i64)> {
        let (servers, pages) = ServerDb::all()
            .paginate(page)
            .per_page(per_page)
            .load_and_count_pages::<ServerDb>(&self.db)
            .await?;
        Ok((servers.into_iter().map(Into::into).collect(), pages))
    }

    pub async fn get_server(&self, sid: Uuid) -> anyhow::Result<Option<ServerWithChannels>> {
        let srv = ServerDb::get(sid, &self.db).await?;
        match srv {
            Some(srv) => {
                let channels = srv.get_channels(&self.db).await?;
                Ok(Some(ServerWithChannels {
                    id: srv.id,
                    name: srv.name,
                    channels: channels.into_iter().map(Into::into).collect(),
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn create_server(&self, name: String) -> anyhow::Result<Uuid> {
        let sid = NewServer::new(name).create(&self.db).await?;
        Ok(sid)
    }

    // FIXME: indicate when creation fails because of unknown sid
    pub async fn create_channel(&self, sid: Uuid, name: String) -> anyhow::Result<Uuid> {
        let chan_id = NewChannel::new(sid, name).create(&self.db).await?;

        Ok(chan_id)
    }

    pub async fn get_channel(
        &self,
        channel_id: Uuid,
    ) -> anyhow::Result<Option<crate::models::Channel>> {
        let c = match ChannelDb::get(channel_id, &self.db).await? {
            None => return Ok(None),
            Some(c) => c,
        };
        let v = match c.assigned_to {
            Some(id) => {
                VoiceServerDb::get_active(id, self.config.voice_server_staleness_duration, &self.db)
                    .await?
                    .map(|v| v.host_url)
            }
            None => {
                tracing::info!("channel currently unassigned");
                None
            }
        };
        let updated_at = c.updated_at;
        let server_id = c.server_id;
        Ok(Some(crate::models::Channel {
            info: c.into(),
            assigned_to: v,
            server_id,
            updated_at,
        }))
    }

    pub async fn assign_channel(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<VoiceServer> {
        let (srv, load) =
            VoiceServerDb::get_smallest_load(self.config.voice_server_staleness_duration, &self.db)
                .await?
                .with_context(|| {
                    tracing::warn!("no voice servers registered");
                    anyhow::anyhow!("no voice servers registered")
                })?;
        tracing::info!(
            "assigned channel to voice server {:?} with load={}",
            srv,
            load
        );
        ChannelDb::assign_to(channel_id, srv.id, reassign, &self.db).await?;
        Ok(srv.into())
    }

    pub async fn unassign_channel(&self, sid: Uuid, channel_id: Uuid) -> anyhow::Result<()> {
        ChannelDb::unassign(channel_id, sid, &self.db).await?;

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn db(&self) -> &Pool {
        &self.db
    }
}

#[derive(Debug, Parser)]
pub struct ChannelsConfig {
    #[clap(
        long,
        env,
        default_value = "postgres://postgres:password@localhost:5432/voices_channels"
    )]
    pub database_url: secrecy::SecretString,
    #[clap(long)]
    pub migrate: bool,
}

impl ChannelsConfig {
    pub async fn server(&self) -> anyhow::Result<ChannelsImpl> {
        let manager = Manager::new(self.database_url.expose_secret(), Runtime::Tokio1);
        let db = Pool::builder(manager).build()?;
        if self.migrate {
            self.migrate(&db).await?;
        }
        let server = ChannelsImpl::new(db);
        Ok(server)
    }

    pub async fn migrate(&self, pool: &Pool) -> anyhow::Result<()> {
        let conn = pool.get().await?;
        use diesel_migrations::MigrationHarness;
        conn.interact(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .map_err(|e| anyhow::anyhow!("migration failed {}", e))?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("Migrations failed"))??;
        Ok(())
    }
}

const MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("./migrations");
