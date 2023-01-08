use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;

use clap::Parser;
use tokio::signal::ctrl_c;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use voice_server::config::VoiceServerConfig;

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
    let ch =
        tonic::transport::Endpoint::from(config.channels_addr.parse::<tonic::transport::Uri>()?)
            .connect_lazy();
    let client = voices_channels::grpc::proto::channels_client::ChannelsClient::new(ch);
    // TODO: register voice server with channels registry, clear previously hosted channels
    let server = config.voice_server.server(Arc::new(client)).await?.grpc();
    tonic::transport::Server::builder()
        .add_service(server)
        .serve_with_shutdown(config.listen_addr(), async {
            tracing::debug!("waiting for shutdown signal");
            let _ = ctrl_c().await;
            tracing::info!("received shutdown signal");
        })
        .await?;
    Ok(())
}

/// Standalone server config
#[derive(clap::Parser, Debug)]
pub struct Config {
    #[clap(long, default_value_t = 33331)]
    http_port: u16,
    #[clap(flatten)]
    voice_server: VoiceServerConfig,
    #[clap(long, default_value = "http://localhost:33330")]
    channels_addr: String,
}

impl Config {
    pub fn listen_addr(&self) -> SocketAddr {
        (Ipv6Addr::UNSPECIFIED, self.http_port).into()
    }
}
