// pub mod rest_api;
pub mod channel_registry;
pub mod server;
mod util;
pub mod voice_instance;

use std::net::SocketAddr;

use axum::extract::ws::WebSocket;
use axum::http::{HeaderValue, Method};
use clap::{CommandFactory, FromArgMatches, Parser};
// use rest_api::{new_channel, servers};
use server::session::ServerSession;
use server::channels::state::ChannelState;
use server::channels::state::distributed::RedisChannelInitializer;
use server::channels::state::local::LocalChannelInitializer;
use tokio::net::TcpListener;
use tracing::subscriber::set_global_default;
// use tracing::Instrument;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;

use crate::channel_registry::{
    distributed::DistributedChannelRegistry, integrated::LocalChannelRegistry, ChannelRegistry,
};
use crate::server::channels::Channels;
use crate::voice_instance::IntegratedVoiceHost;

#[derive(Parser)]
#[clap(name = "voice-server")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value_t = 33332, env)]
    http_listen_port: u16,
    #[clap(subcommand)]
    setup: SetupSubcommand,
}

#[derive(Debug, Parser)]
enum SetupSubcommand {
    Distributed {
        #[clap(long, default_value = "http://localhost:33330", env)]
        channels_addr: String,
        #[clap(long, default_value = "redis://127.0.0.1:6379/", env)]
        redis_conn: String,
    },
    #[cfg(feature = "standalone")]
    Standalone {
        #[clap(flatten)]
        voice_config: voice_server::config::VoiceServerConfig,
        #[clap(flatten)]
        channels_config: voices_channels::ChannelsConfig,
    },
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
    let matches = Config::into_app()
        .dont_collapse_args_in_usage(true)
        .get_matches();
    let config = Config::from_arg_matches(&matches)?;
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], config.http_listen_port));
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    tracing::info!("Control listening on {}", ctl_listener.local_addr()?);
    match config.setup {
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
    R: ChannelRegistry + Clone,
    S: ChannelState + Clone,
{
    use tower_http::cors::CorsLayer;
    let router = axum::Router::new()
        .route("/ws", axum::routing::get(websocket_handler))
        // .route("/channels", axum::routing::post(new_channel))
        // .route("/channels/:id", axum::routing::get(rest_api::get_channel))
        // .route("/servers", axum::routing::get(servers))
        // .route("/servers/:id", axum::routing::get(rest_api::get_server))
        // .route("/servers", axum::routing::post(rest_api::new_server))
        // .route(
        //     "/voice_servers/cleanup",
        //     axum::routing::post(rest_api::cleanup_stale_voice_servers),
        // )
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET, Method::POST]),
        )
        .with_state(AppState { channels });
    let srv = axum::Server::from_tcp(ctl_listener.into_std()?)?.serve(router.into_make_service());
    srv.await?;
    Ok(())
    // loop {
    //     tracing::debug!("waiting for tcp connection");
    //     let (inc, addr) = ctl_listener.accept().await?;
    //     tracing::debug!("accepted tcp connection from {}", addr);
    //     let channels = channels.clone();
    //     let span = tracing::span!(tracing::Level::INFO, "session", addr=%addr,);
    //     tokio::spawn(
    //         async move {
    //             if let Err(e) = handle_session(inc, channels).await {
    //                 tracing::warn!("session ended with error: {}", e);
    //             } else {
    //                 tracing::info!("session ended");
    //             }
    //         }
    //         .instrument(span),
    //     );
    // }
}

#[derive(Clone)]
pub struct AppState<S, R>
where
    R: ChannelRegistry,
    S: ChannelState,
{
    channels: Channels<S, R>,
}

async fn websocket_handler<S, R>(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState<S, R>>,
) -> impl axum::response::IntoResponse
where
    R: ChannelRegistry,
    S: ChannelState,
{
    ws.on_upgrade(|socket| async move {
        if let Err(e) = handle_session(socket, state.channels).await {
            tracing::warn!("session ended with error: {}", e);
        } else {
            tracing::info!("session ended");
        }
    })
}

async fn handle_session<R, S>(socket: WebSocket, channels: Channels<S, R>) -> anyhow::Result<()>
where
    R: ChannelRegistry,
    S: ChannelState,
{
    ServerSession::init(socket, channels)
        .await?
        .run_session()
        .await?;
    Ok(())
}
