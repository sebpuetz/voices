pub mod ports;
pub mod server;
mod util;
pub mod voice;

use std::net::SocketAddr;

use clap::Parser;
use ports::Ports;
use server::session::ServerSession;
use tokio::net::TcpStream;
use tracing::Instrument;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;

use crate::server::channel::Channels;
use crate::server::session::UdpHostInfo;

#[derive(Parser)] // requires `derive` feature
#[clap(name = "voice-client")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value_t = 33335)]
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
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let config = Config::parse();
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], config.ws_listen_port));
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    tracing::info!("Control listening on {}", ctl_listener.local_addr()?);
    let channels = Channels::new();
    let ports = Ports::new(config.first_udp_port, config.udp_ports as _);
    let host_info = UdpHostInfo::Host(config.udp_host);

    loop {
        tracing::debug!("waiting for tcp connection");
        let (inc, addr) = ctl_listener.accept().await?;
        tracing::debug!("accepted tcp connection from {}", addr);
        let channels = channels.clone();
        let ports = ports.clone();
        let span = tracing::span!(tracing::Level::INFO, "session", addr=%addr,);
        let host_info = host_info.clone();
        tokio::spawn(
            async move {
                if let Err(e) = handle_session(inc, channels, ports, host_info).await {
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
    ports: Ports,
    host_info: UdpHostInfo,
) -> anyhow::Result<()> {
    ServerSession::init(inc, ports, channels, host_info)
        .await?
        .run_session()
        .await?;
    Ok(())
}
