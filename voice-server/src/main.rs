use std::sync::Arc;

use clap::Parser;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use uuid::Uuid;
use voice_server::registry::Registry;
use voice_server::server::{self, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    set_global_default(
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
            ))
            .with(tracing_subscriber::fmt::layer()),
    )?;
    LogTracer::init()?;
    let config = Config::parse();
    tracing::info!("starting server with {:#?}", config);
    let endpoint = config.channels_addr.parse::<tonic::transport::Uri>()?;
    tracing::info!(endpoint=%endpoint, "setting up remote channel registry client");

    let ch = tonic::transport::Endpoint::from(endpoint).connect_lazy();
    let client = voices_channels::grpc::proto::channels_client::ChannelsClient::new(ch);
    let server_id = config.server_id.unwrap_or_else(Uuid::new_v4);
    let registry = Registry::new(Arc::new(client), server_id);
    let srv = server::serve(
        Arc::new(registry),
        config.listen_addr(),
        format!("{}:{}", config.http_host_url, config.http_port),
        config.voice_server,
    )
    .await?;
    srv.wait().await?;
    Ok(())
}
