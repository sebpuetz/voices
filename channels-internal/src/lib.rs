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
#[cfg(test)]
pub mod test_helper;

#[derive(Debug, Parser)]
pub struct ChannelsConfig {
    #[clap(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://postgres:password@localhost:5432/voices_channels"
    )]
    pub database_url: secrecy::SecretString,
}

impl ChannelsConfig {
    pub fn server(&self) -> anyhow::Result<ChannelsImpl> {
        let manager = Manager::new(self.database_url.expose_secret(), Runtime::Tokio1);
        let db = Pool::builder(manager).build()?;

        let server = grpc::service::ChannelsImpl::new(db);
        Ok(server)
    }
}
