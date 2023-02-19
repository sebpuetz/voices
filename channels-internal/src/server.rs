use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio::signal::ctrl_c;
use tokio_stream::wrappers::TcpListenerStream;
use tower::ServiceBuilder;
use tower_http::classify::{GrpcCode, GrpcErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use crate::ChannelsConfig;

pub async fn server(addr: SocketAddr, cfg: ChannelsConfig) -> anyhow::Result<Server> {
    let service = cfg.server().await?.grpc();

    let classifier = GrpcErrorsAsFailures::new()
        .with_success(GrpcCode::InvalidArgument)
        .with_success(GrpcCode::NotFound);
    let layers = ServiceBuilder::new()
        .layer(
            TraceLayer::new(SharedClassifier::new(classifier))
                .make_span_with(DefaultMakeSpan::new().include_headers(true)),
        )
        .into_inner();
    tracing::info!("Running channels with {:#?}", cfg);
    let incoming = TcpListener::bind(addr).await?;
    let local_addr = incoming.local_addr()?;
    tracing::info!("locally listening on {}", local_addr);
    let srv = tonic::transport::Server::builder()
        .layer(layers)
        .add_service(service)
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
    pub async fn wait(self) -> anyhow::Result<()> {
        self.handle.await??;
        Ok(())
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}
