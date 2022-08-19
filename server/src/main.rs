pub mod ports;
pub mod server;
mod util;
pub mod voice;

use std::net::SocketAddr;

use ports::Ports;
use server::session::ServerSession;
use server::voice::Channels;
use tokio::net::TcpStream;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], 33335));
    rt.block_on(main_(tcp_addr)).map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
    Ok(())
}

async fn main_(tcp_addr: SocketAddr) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    tracing::info!("Control listening on {}", ctl_listener.local_addr()?);
    let channels = Channels::new();
    let ports = Ports::new(33333, 2);
    loop {
        tracing::debug!("waiting for tcp connection");
        let (inc, addr) = ctl_listener.accept().await?;
        tracing::debug!("accepted tcp connection from {}", addr);
        let channels = channels.clone();
        let ports = ports.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_session(inc, channels, ports).await {
                tracing::warn!("{}", e);
            }
        });
    }
}

async fn handle_session(inc: TcpStream, channels: Channels, ports: Ports) -> anyhow::Result<()> {
    ServerSession::await_handshake(inc, ports, channels)
        .await?
        .run_session()
        .await?;
    Ok(())
}
