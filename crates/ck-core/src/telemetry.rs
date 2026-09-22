//! Logging setup shared by the Tauri shell and `ck-serve`.
//!
//! With the `otel` feature (dev only, never in release builds) spans also go to
//! an OTLP backend: `OTEL_EXPORTER_OTLP_ENDPOINT` if set, else the Logfire
//! project in `.logfire/`. The `ck_llm_trace` target (full prompts and
//! responses) reaches only that exporter, and only with `CK_OTEL_CONTENT` set.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[cfg(all(feature = "otel", not(debug_assertions)))]
compile_error!("the `otel` feature exports prompts and must not ship in release builds");

pub const LLM_TRACE: &str = "ck_llm_trace";

/// Flushes pending spans on drop. Keep it alive for the life of the process.
pub struct Guard {
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(p) = self.provider.take() {
            let _ = p.shutdown();
        }
    }
}

pub fn init(default_filter: &str) -> Guard {
    let fmt_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into())
        .add_directive(
            format!("{LLM_TRACE}=off")
                .parse()
                .expect("static directive"),
        );
    let fmt = tracing_subscriber::fmt::layer().with_filter(fmt_filter);

    #[cfg(feature = "otel")]
    {
        let (otel, provider) = match otel_layer() {
            Some((layer, provider)) => (Some(layer), Some(provider)),
            None => (None, None),
        };
        tracing_subscriber::registry().with(fmt).with(otel).init();
        if provider.is_some() {
            tracing::info!(
                content = std::env::var_os("CK_OTEL_CONTENT").is_some(),
                "OTLP tracing enabled"
            );
        }
        Guard { provider }
    }
    #[cfg(not(feature = "otel"))]
    {
        tracing_subscriber::registry().with(fmt).init();
        Guard {}
    }
}

#[cfg(feature = "otel")]
fn otel_layer<S>() -> Option<(impl Layer<S>, opentelemetry_sdk::trace::SdkTracerProvider)>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};

    let builder = opentelemetry_otlp::SpanExporter::builder().with_http();
    let builder = if std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some() {
        builder
    } else {
        let (url, token) = logfire_credentials()?;
        builder
            .with_endpoint(format!("{}/v1/traces", url.trim_end_matches('/')))
            .with_headers(std::collections::HashMap::from([(
                "Authorization".to_string(),
                token,
            )]))
    };
    let exporter = match builder.build() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("OTLP exporter not started: {e}");
            return None;
        }
    };
    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name("chronicle-keeper")
        .with_attribute(opentelemetry::KeyValue::new(
            "deployment.environment.name",
            "dev",
        ))
        .build();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();
    // Prompts and responses hold transcripts and player names: opt in per run.
    let content = if std::env::var_os("CK_OTEL_CONTENT").is_some() {
        "trace"
    } else {
        "off"
    };
    let filter = EnvFilter::try_from_env("CK_OTEL_FILTER").unwrap_or_else(|_| {
        format!("ck_core=debug,{LLM_TRACE}={content},tower_http=debug,warn").into()
    });
    let layer = tracing_opentelemetry::layer()
        .with_tracer(provider.tracer("chronicle-keeper"))
        .with_filter(filter);
    Some((layer, provider))
}

/// `.logfire/logfire_credentials.json` from `logfire init use`, searched from the
/// working directory upwards so both `ck-serve` and `cargo tauri dev` find it.
#[cfg(feature = "otel")]
fn logfire_credentials() -> Option<(String, String)> {
    let cwd = std::env::current_dir().ok()?;
    let file = cwd
        .ancestors()
        .map(|d| d.join(".logfire/logfire_credentials.json"))
        .find(|p| p.is_file())?;
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    Some((
        v.get("logfire_api_url")?.as_str()?.to_string(),
        v.get("token")?.as_str()?.to_string(),
    ))
}
