use clap::Parser;
use deadpool::Runtime;
use deadpool_diesel::postgres::{Manager, Pool};
use grpc::service::ChannelsImpl;
use secrecy::ExposeSecret;

pub mod db;
pub mod error;
pub mod models;
pub mod schema;

pub mod grpc;
pub mod server;
#[cfg(test)]
mod test;
#[cfg(any(test, feature = "test"))]
pub mod test_helper;

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
        let server = grpc::service::ChannelsImpl::new(db);
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
