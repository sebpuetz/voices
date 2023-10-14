use clap::{CommandFactory, FromArgMatches};
// use tracing::subscriber::set_global_default;
// use tracing_log::LogTracer;
// use tracing_subscriber::prelude::*;
use voices_gateway::{run, Config};

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    // set_global_default(
    //     tracing_subscriber::registry()
    //         .with(tracing_subscriber::EnvFilter::new(
    //             std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
    //         ))
    //         .with(
    //             tracing_subscriber::fmt::layer()
    //                 .with_line_number(true)
    //                 .with_file(true),
    //         ),
    // )?;
    // LogTracer::init()?;
    let matches = Config::into_app()
        .dont_collapse_args_in_usage(true)
        .get_matches();
    let config = Config::from_arg_matches(&matches)?;
    rt.block_on(async move {
        voices_tracing::init("gateway", Some("http://host.docker.internal:4317".into()))?;
        run(config).await
    })
    .map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
    Ok(())
}
