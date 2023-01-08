use std::net::SocketAddr;

use clap::Parser;
use tokio::signal::ctrl_c;
use tower::ServiceBuilder;
use tower_http::classify::{GrpcCode, GrpcErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
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
    let service = cfg.channels.server()?.grpc();

    let classifier = GrpcErrorsAsFailures::new()
        .with_success(GrpcCode::InvalidArgument)
        .with_success(GrpcCode::NotFound);
    let layers = ServiceBuilder::new()
        .layer(
            TraceLayer::new(SharedClassifier::new(classifier))
                .make_span_with(DefaultMakeSpan::new().include_headers(true)),
        )
        .into_inner();
    tracing::info!("Running channels on {:#?}", cfg);
    tonic::transport::Server::builder()
        .layer(layers)
        .add_service(service)
        .serve_with_shutdown(SocketAddr::from(([127, 0, 0, 1], cfg.listen_port)), async {
            tracing::debug!("waiting for shutdown signal");
            let _ = ctrl_c().await;
            tracing::info!("received shutdown signal");
        })
        .await?;
    Ok(())
}
