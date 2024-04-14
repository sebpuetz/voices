pub mod registry;
mod server;
pub mod service;

use clap::Parser;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use uuid::Uuid;

use crate::registry::voice_channels::channels_client::ChannelsClient;
use crate::registry::Registry;
use crate::server::Config;

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
    let client = ChannelsClient::new(ch);
    let server_id = config.server_id.unwrap_or_else(Uuid::new_v4);
    let registry = Registry::new_client(client, server_id);
    let srv = server::serve(
        registry,
        config.listen_addr(),
        format!("{}:{}", config.http_host_url, config.http_port),
        config.voice_server,
    )
    .await?;
    srv.wait().await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;
    use std::time::Duration;

    use assert_matches::assert_matches;
    use async_trait::async_trait;
    use uuid::Uuid;
    use voices_voice::channel::Channel;
    use voices_voice::config::VoiceServerConfig;
    use voices_voice::{StatusError, VoiceServerImpl};

    use crate::registry::{MockRegister, Register, Registry};
    use crate::server::Server;

    #[derive(Clone, Debug, Default)]
    pub struct CloneableMockRegister {
        inner: Arc<tokio::sync::Mutex<MockRegister>>,
    }

    impl CloneableMockRegister {
        pub fn new(mock: MockRegister) -> Self {
            Self {
                inner: Arc::new(tokio::sync::Mutex::new(mock)),
            }
        }

        pub async fn checkpoint(&self) {
            self.inner.lock().await.checkpoint();
        }
    }

    #[async_trait]
    impl Register for CloneableMockRegister {
        async fn register(&self, addr: String) -> anyhow::Result<()> {
            self.inner.lock().await.register(addr).await
        }
        async fn unassign_channel(&self, channel_id: Uuid) -> anyhow::Result<()> {
            self.inner.lock().await.unassign_channel(channel_id).await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_register_and_heartbeat() {
        init_tracing();
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));
        let voice_cfg = VoiceServerConfig {
            first_udp_port: 0,
            udp_ports: 5,
            udp_host: "localhost".into(),
        };
        let mut registry = MockRegister::new();
        let n = Arc::new(AtomicU8::default());
        registry.expect_register().times(3..).returning(move |_| {
            let n = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 1 {
                Box::pin(async { Err(anyhow::anyhow!("oops")) })
            } else {
                Box::pin(async { Ok(()) })
            }
        });
        let registry = CloneableMockRegister::new(registry);

        crate::server::serve(
            Registry::new(registry.clone()),
            addr,
            format!("http://localhost:{}", addr.port()),
            voice_cfg,
        )
        .await
        .unwrap();
        tokio::time::advance(Server::HEARTBEAT_INTERVAL + Duration::from_secs(1)).await;
        tokio::time::advance(Server::HEARTBEAT_INTERVAL + Duration::from_secs(1)).await;
        tokio::time::advance(Server::HEARTBEAT_INTERVAL + Duration::from_secs(1)).await;
        registry.checkpoint().await;
    }

    #[tokio::test(start_paused = true)]
    async fn test_chan_drop() {
        let mut registry = MockRegister::new();
        registry
            .expect_unassign_channel()
            .once()
            .return_once(move |_| Box::pin(async { Ok(()) }));
        let registry = CloneableMockRegister::new(registry);
        let srv = create_standalone_srv(registry.clone()).await.unwrap();
        let id = Uuid::new_v4();
        srv.assign_channel(id).await.unwrap();
        assert_matches!(srv.get_channel(id).await, Some(_));
        tokio::time::sleep(Channel::CHANNEL_IDLE_TIMEOUT + Duration::from_secs(1)).await;
        assert_matches!(srv.get_channel(id).await, None);
        assert_matches!(srv.status(id).await, Err(StatusError::ChannelNotFound(_)));
        registry.checkpoint().await;
    }

    async fn create_standalone_srv(
        registry: CloneableMockRegister,
    ) -> anyhow::Result<VoiceServerImpl> {
        init_tracing();
        let voice_cfg = VoiceServerConfig {
            first_udp_port: 0,
            udp_ports: 5,
            udp_host: "localhost".into(),
        };
        voice_cfg
            .server_with_end_notify(Registry::new(registry))
            .await
    }

    fn init_tracing() {
        let v = std::env::var("TEST_LOG").ok().is_some();
        use tracing_subscriber::prelude::*;
        if v {
            let _ = tracing::subscriber::set_global_default(
                tracing_subscriber::registry()
                    .with(tracing_subscriber::EnvFilter::new(
                        std::env::var("RUST_LOG").unwrap_or_else(|_| "DEBUG".into()),
                    ))
                    .with(tracing_subscriber::fmt::layer()),
            );
            let _ = tracing_log::LogTracer::init();
        }
    }
}
