pub mod channel_registry;
pub mod rest_api;
pub mod server;
mod util;
pub mod voice_instance;

use std::net::SocketAddr;

use axum::extract::ws::WebSocket;
use axum::http::{HeaderValue, Method};
use channel_registry::ChannelRegistry;
use clap::Parser;
use server::channels::state::ChannelState;
use server::session::ServerSession;
#[cfg(feature = "distributed")]
mod distributed {
    pub(super) use crate::channel_registry::distributed::DistributedChannelRegistry;
    pub(super) use crate::server::channels::state::distributed::RedisChannelInitializer;
}
#[cfg(feature = "distributed")]
use distributed::*;
#[cfg(feature = "standalone")]
mod standalone {
    pub(super) use crate::channel_registry::integrated::LocalChannelRegistry;
    pub(super) use crate::server::channels::state::local::LocalChannelInitializer;
    pub(super) use crate::voice_instance::IntegratedVoiceHost;
}
#[cfg(feature = "standalone")]
use standalone::*;
use tokio::net::TcpListener;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::channel_registry::GetVoiceHost;
use crate::server::channels::Channels;

#[derive(Parser)]
#[clap(name = "voice-server")]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    #[clap(long, default_value_t = 33332, env)]
    http_listen_port: u16,
    #[clap(subcommand)]
    setup: SetupSubcommand,
}

#[derive(Debug, Parser)]
pub enum SetupSubcommand {
    #[cfg(feature = "distributed")]
    Distributed {
        #[clap(long, default_value = "http://localhost:33330", env)]
        channels_addr: String,
        #[clap(long, default_value = "redis://127.0.0.1:6379/", env)]
        redis_conn: String,
    },
    #[cfg(feature = "standalone")]
    Standalone {
        #[clap(flatten)]
        voice_config: voices_voice::config::VoiceServerConfig,
        #[clap(flatten)]
        channels_config: voices_channels::ChannelsConfig,
    },
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], config.http_listen_port));
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    tracing::info!("Control listening on {}", ctl_listener.local_addr()?);
    match config.setup {
        #[cfg(feature = "distributed")]
        SetupSubcommand::Distributed {
            channels_addr,
            redis_conn,
        } => {
            let mgr = deadpool_redis::Manager::new(redis_conn)?;
            let pool = deadpool_redis::Pool::builder(mgr).build()?;
            let room_init = RedisChannelInitializer::new(pool);
            let registry = DistributedChannelRegistry::new(channels_addr.parse()?);
            let channels = Channels::new(room_init, registry);

            serve(ctl_listener, channels).await
        }
        #[cfg(feature = "standalone")]
        SetupSubcommand::Standalone {
            voice_config,
            channels_config,
        } => {
            let channels_impl = channels_config.server().await?;
            let voice = IntegratedVoiceHost::new(voice_config.server().await?);
            let local_registry = LocalChannelRegistry::new(channels_impl, voice);
            let channels = Channels::new(LocalChannelInitializer, local_registry);
            serve(ctl_listener, channels).await
        }
    }
}

async fn serve<S, R>(ctl_listener: TcpListener, channels: Channels<S, R>) -> anyhow::Result<()>
where
    R: GetVoiceHost + ChannelRegistry + Clone,
    S: ChannelState + Clone,
{
    use tower_http::cors::CorsLayer;
    let router = axum::Router::new()
        .route("/ws", axum::routing::get(websocket_handler))
        .route("/channels", axum::routing::post(rest_api::new_channel))
        .route("/channels/:id", axum::routing::get(rest_api::get_channel))
        .route("/servers", axum::routing::get(rest_api::servers))
        .route("/servers/:id", axum::routing::get(rest_api::get_server))
        .route("/servers", axum::routing::post(rest_api::new_server))
        .route(
            "/voice_servers/cleanup",
            axum::routing::post(rest_api::cleanup_stale_voice_servers),
        )
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET, Method::POST]),
        )
        .with_state(AppState { channels });
    let mut cfg = server_framework::Config::default();
    // cfg.bind_address = SocketAddr::from(([0, 0, 0, 0], config.http_listen_port));
    cfg.metrics_health_port = 9000;
    let srv = server_framework::Server::new(cfg)
        .always_live_and_ready()
        .with(router)
        .serve_with_listener(ctl_listener);
    // let srv = axum::Server::from_tcp(ctl_listener.into_std()?)?.serve(router.into_make_service());
    srv.await?;
    Ok(())
}

#[derive(Clone)]
pub struct AppState<S, R> {
    channels: Channels<S, R>,
}

async fn websocket_handler<S, R>(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState<S, R>>,
) -> impl axum::response::IntoResponse
where
    R: GetVoiceHost,
    S: ChannelState,
{
    let span = tracing::Span::current();
    let ctx = span.context();
    let ws_span = tracing::info_span!(parent: None, "websocket");
    ws_span.set_parent(ctx);
    ws.on_upgrade(move |socket| {
        async move {
            if let Err(e) = handle_session(socket, state.channels).await {
                tracing::warn!("session ended with error: {}", e);
            } else {
                tracing::info!("session ended");
            }
        }
        .instrument(ws_span)
    })
}

async fn handle_session<R, S>(socket: WebSocket, channels: Channels<S, R>) -> anyhow::Result<()>
where
    R: GetVoiceHost,
    S: ChannelState,
{
    ServerSession::init_websocket(socket, channels)
        .await?
        .run_session()
        .await?;
    Ok(())
}

#[cfg(test)]
fn test_log() {
    use tracing_subscriber::prelude::*;
    if std::env::var("TEST_LOG").is_ok() {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(tracing_subscriber::EnvFilter::new(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
                ))
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_line_number(true)
                        .with_file(true),
                ),
        );
        let _ = tracing_log::LogTracer::init();
    }
}
