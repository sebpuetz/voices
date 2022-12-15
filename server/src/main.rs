pub mod server;
mod util;

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;
use server::session::ServerSession;
use tokio::net::TcpStream;
use tracing::subscriber::set_global_default;
use tracing::Instrument;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use voice_server::channel::Channels;
use voice_server::{Ports, VoiceServer};

#[derive(Parser)]
#[clap(name = "voice-server")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value_t = 33332)]
    ws_listen_port: u16,
    #[clap(long, default_value_t = 33333)]
    first_udp_port: u16,
    #[clap(long, default_value_t = 2)]
    udp_ports: u16,
    #[clap(long, default_value = "localhost")]
    udp_host: String,
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
    let ports = Ports::new(config.first_udp_port, config.udp_ports as _);
    let host_addr = tokio::net::lookup_host((&*config.udp_host, 0))
        .await
        .context("Failed to resolve UDP host")?
        .next()
        .context("Failed to resolve UDP host")?
        .ip();
    let voice_server = VoiceServer::new(host_addr, ports);

    loop {
        tracing::debug!("waiting for tcp connection");
        let (inc, addr) = ctl_listener.accept().await?;
        tracing::debug!("accepted tcp connection from {}", addr);
        let channels = channels.clone();
        let span = tracing::span!(tracing::Level::INFO, "session", addr=%addr,);
        let voice_server = voice_server.clone();
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

async fn handle_session(
    inc: TcpStream,
    channels: Channels,
    voice_server: VoiceServer,
) -> anyhow::Result<()> {
    ServerSession::init(inc, voice_server, channels)
        .await?
        .run_session()
        .await?;
    Ok(())
}
