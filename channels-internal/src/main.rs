use std::net::SocketAddr;

use clap::Parser;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use voices_channels::server::server;
use voices_channels::ChannelsConfig;

#[derive(Debug, Parser)]
#[clap(name = "voice-channels")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value_t = 33330)]
    pub listen_port: u16,
    #[clap(flatten)]
    pub channels: ChannelsConfig,
}

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
    let cfg = Config::parse();
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.listen_port));
    let srv = server(addr, cfg.channels).await?;
    srv.wait().await
}
