pub mod server;
mod util;

use std::net::SocketAddr;

use clap::Parser;
use server::session::ServerSession;
use tokio::net::{TcpListener, TcpStream};
use tracing::subscriber::set_global_default;
use tracing::Instrument;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use voice_server::config::VoiceServerConfig;
use voice_server::VoiceServer;

use crate::server::channels::Channels;

#[derive(Parser)]
#[clap(name = "voice-server")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value_t = 33332)]
    ws_listen_port: u16,
    // FIXME: voice_config and voice_server_addr should be mutually exclusive...
    #[clap(long)]
    voice_server_addr: Option<String>,
    #[clap(flatten)]
    voice_config: VoiceServerConfig,
}

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(main_()).map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
    Ok(())
}

async fn main_() -> anyhow::Result<()> {
    set_global_default(
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
            ))
            .with(tracing_subscriber::fmt::layer()),
    )?;
    LogTracer::init()?;
    let config = Config::parse();
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], config.ws_listen_port));
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    tracing::info!("Control listening on {}", ctl_listener.local_addr()?);
    let channels = Channels::new();
    match config.voice_server_addr {
        Some(addr) => {
            // FIXME lazy connect
            let client =
                voice_server::grpc::proto::voice_server_client::VoiceServerClient::connect(addr)
                    .await?;
            run_server(client, ctl_listener, channels).await
        }
        None => run_server(config.voice_config.server().await?, ctl_listener, channels).await,
    }
}

async fn run_server<V: VoiceServer + Clone>(
    voice_handle: V,
    ctl_listener: TcpListener,
    channels: Channels,
) -> anyhow::Result<()> {
    loop {
        tracing::debug!("waiting for tcp connection");
        let (inc, addr) = ctl_listener.accept().await?;
        tracing::debug!("accepted tcp connection from {}", addr);
        let channels = channels.clone();
        let span = tracing::span!(tracing::Level::INFO, "session", addr=%addr,);
        let voice_server = voice_handle.clone();
        tokio::spawn(
            async move {
                if let Err(e) = handle_session(inc, channels, voice_server).await {
                    tracing::warn!("session ended with error: {}", e);
                } else {
                    tracing::info!("session ended");
                }
            }
            .instrument(span),
        );
    }
}

async fn handle_session<V>(
    inc: TcpStream,
    channels: Channels,
    voice_server: V,
) -> anyhow::Result<()>
where
    V: VoiceServer + Clone,
{
    ServerSession::init(inc, voice_server, channels)
        .await?
        .run_session()
        .await?;
    Ok(())
}
