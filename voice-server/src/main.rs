use std::net::{Ipv6Addr, SocketAddr};

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
    let server = config.voice_server.server().await?.grpc();
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
}

impl Config {
    pub fn listen_addr(&self) -> SocketAddr {
        (Ipv6Addr::UNSPECIFIED, self.http_port).into()
    }
}
