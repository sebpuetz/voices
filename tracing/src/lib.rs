use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::sdk::trace;
use opentelemetry::sdk::trace::{RandomIdGenerator, Sampler};
use opentelemetry::sdk::Resource;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;

pub fn init(service_name: &str, otlp_uri: Option<String>) -> anyhow::Result<()> {
    LogTracer::init()?;
    let opentelemetry_layer;
    if let Some(uri) = otlp_uri {
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(uri);

        let trace_config = trace::config()
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
            .with_id_generator(RandomIdGenerator::default())
            .with_resource(Resource::new(vec![KeyValue::new(
                "service.name",
                service_name.to_owned(),
            )]));
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(trace_config)
            .install_batch(opentelemetry::runtime::Tokio)
            .map(|tracer| {
                tracing_opentelemetry::layer().with_tracer(tracer)
                // add filter here for otel
                // .with_filter(tracing_subscriber::EnvFilter::new(
                //     std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
                // ))
            })?;
        opentelemetry_layer = Some(tracer);
    } else {
        opentelemetry_layer = None;
    };

    let stdout = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true);
    set_global_default(
        tracing_subscriber::Registry::default()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
            ))
            .with(opentelemetry_layer)
            .with(stdout),
    )?;
    Ok(())
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
