use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::signal::ctrl_c;
use tokio_stream::wrappers::TcpListenerStream;
use uuid::Uuid;

use crate::config::VoiceServerConfig;
use crate::registry::Register;

pub async fn serve(
    registry: Arc<dyn Register>,
    listen_addr: SocketAddr,
    external_http_addr: String,
    config: VoiceServerConfig,
) -> anyhow::Result<Server> {
    let incoming = TcpListener::bind(listen_addr).await?;
    serve_with_incoming(registry, incoming, external_http_addr, config).await
}

pub async fn serve_with_incoming(
    registry: Arc<dyn Register>,
    incoming: TcpListener,
    external_http_addr: String,
    config: VoiceServerConfig,
) -> anyhow::Result<Server> {
    let server = config.server_with_registry(registry.clone()).await?.grpc();
    let local_addr = incoming.local_addr()?;
    // TODO: register voice server with channels registry, clear previously hosted channels
    tokio::spawn(heartbeat(registry, external_http_addr).await?);
    tracing::info!("locally listening on {}", local_addr);
    let srv = tonic::transport::Server::builder()
        .add_service(server)
        .serve_with_incoming_shutdown(TcpListenerStream::new(incoming), async {
            tracing::debug!("waiting for shutdown signal");
            let _ = ctrl_c().await;
            tracing::info!("received shutdown signal");
        });
    Ok(Server {
        handle: tokio::spawn(srv),
        addr: local_addr,
    })
}

pub struct Server {
    handle: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    addr: SocketAddr,
}

impl Server {
    pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

    pub async fn wait(self) -> anyhow::Result<()> {
        self.handle.await??;
        Ok(())
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

async fn heartbeat(
    channels: Arc<dyn Register>,
    addr: String,
) -> anyhow::Result<impl std::future::Future<Output = ()>> {
    channels.register(addr.clone()).await?;
    tracing::info!("registered server");
    Ok(async move {
        let mut interval = tokio::time::interval(Server::HEARTBEAT_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(e) = channels.register(addr.clone()).await {
                tracing::warn!(error = %e, "failed to refresh server registration");
            } else {
                tracing::info!("refreshed server registration");
            }
        }
    })
}

/// Standalone server config
#[derive(clap::Parser, Debug)]
pub struct Config {
    #[clap(long, default_value = "http://localhost", env)]
    pub http_host_url: String,
    #[clap(long, default_value_t = 33331, env)]
    pub http_port: u16,
    #[clap(flatten)]
    pub voice_server: VoiceServerConfig,
    #[clap(long, default_value = "http://localhost:33330", env)]
    pub channels_addr: String,
    #[clap(long, env)]
    pub server_id: Option<Uuid>,
}

impl Config {
    pub fn listen_addr(&self) -> SocketAddr {
        (Ipv6Addr::UNSPECIFIED, self.http_port).into()
    }
}
