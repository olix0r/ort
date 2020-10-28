#![deny(warnings, rust_2018_idioms)]
#![allow(warnings)]

mod admin;
mod grpc;
mod metrics;
mod rate_limit;
mod report;
mod runner;

use self::{
    admin::Admin, grpc::MakeGrpc, metrics::MakeMetrics, rate_limit::RateLimit, runner::Runner,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use structopt::StructOpt;
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::RwLock,
};
use tokio_compat_02::FutureExt;
use tracing::debug_span;
use tracing_futures::Instrument;

mod proto {
    tonic::include_proto!("ortiofay.olix0r.net");
}

#[async_trait::async_trait]
pub trait MakeClient {
    type Client: Client;

    async fn make_client(&mut self) -> Self::Client;
}

#[async_trait::async_trait]
pub trait Client {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status>;
}

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {
    #[structopt(short, long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(long, default_value = "0")]
    request_limit: usize,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "1s")]
    request_limit_window: Duration,

    #[structopt(long)]
    total_requests: Option<usize>,

    // #[structopt(long)]
    // total_requests: Option<usize>,
    #[structopt(short, long, default_value = "1")]
    clients: usize,

    #[structopt(short, long, default_value = "1")]
    streams: usize,

    #[structopt(parse(try_from_str = Target::parse))]
    target: Target,
}

#[derive(Clone, Debug)]
enum Target {
    Grpc(http::Uri),
}

impl Load {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let Self {
            admin_addr,
            clients,
            streams,
            request_limit,
            request_limit_window,
            total_requests,
            target: Target::Grpc(target),
        } = self;

        if clients == 0 || streams == 0 {
            return Ok(());
        }

        let histogram = Arc::new(RwLock::new(hdrhistogram::Histogram::new(3).unwrap()));
        let admin = Admin::new(histogram.clone());
        tokio::spawn(async move {
            let connect = MakeGrpc::new(target, Duration::from_secs(1));
            let connect = MakeMetrics::new(connect, histogram);
            let limit = RateLimit::new(request_limit, request_limit_window);
            Runner::new(clients, streams, total_requests.unwrap_or(0), limit)
                .run(connect)
                .await
        });

        tokio::spawn(
            async move {
                admin
                    .serve(admin_addr)
                    .compat()
                    .await
                    .expect("Admin server must not fail")
            }
            .instrument(debug_span!("admin")),
        );

        signal(SignalKind::terminate())?.recv().await;

        Ok(())
    }
}

// === impl Target ===

impl Target {
    fn parse(s: &str) -> Result<Target, Box<dyn std::error::Error + 'static>> {
        use std::str::FromStr;

        let uri = http::Uri::from_str(s)?;
        match uri.scheme_str() {
            Some("grpc") | None => Ok(Target::Grpc(uri)),
            Some(scheme) => Err(UnsupportedScheme(scheme.to_string()).into()),
        }
    }
}

#[derive(Debug)]
struct UnsupportedScheme(String);

impl std::fmt::Display for UnsupportedScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unsupported scheme: {}", self.0)
    }
}

impl std::error::Error for UnsupportedScheme {}

// === parse_duration ===

fn parse_duration(s: &str) -> Result<Duration, InvalidDuration> {
    use regex::Regex;

    let re = Regex::new(r"^\s*(\d+)(ms|s|m|h|d)?\s*$").expect("duration regex");
    let cap = re.captures(s).ok_or(InvalidDuration)?;
    let magnitude = cap[1].parse().map_err(|_| InvalidDuration)?;
    match cap.get(2).map(|m| m.as_str()) {
        None if magnitude == 0 => Ok(Duration::from_secs(0)),
        Some("ms") => Ok(Duration::from_millis(magnitude)),
        Some("s") => Ok(Duration::from_secs(magnitude)),
        Some("m") => Ok(Duration::from_secs(magnitude * 60)),
        Some("h") => Ok(Duration::from_secs(magnitude * 60 * 60)),
        Some("d") => Ok(Duration::from_secs(magnitude * 60 * 60 * 24)),
        _ => Err(InvalidDuration),
    }
}

#[derive(Copy, Clone, Debug)]
struct InvalidDuration;

impl std::fmt::Display for InvalidDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid duration")
    }
}

impl std::error::Error for InvalidDuration {}
