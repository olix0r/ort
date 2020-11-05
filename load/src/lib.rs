#![deny(warnings, rust_2018_idioms)]

mod admin;
mod distribution;
mod grpc;
mod http;
mod limit;
mod metrics;
mod rate_limit;
mod report;
mod runner;
mod timeout;

mod proto {
    tonic::include_proto!("ort.olix0r.net");
}

use self::{
    admin::Admin, distribution::Distribution, grpc::MakeGrpc, http::MakeHttp, metrics::MakeMetrics,
    rate_limit::RateLimit, runner::Runner, timeout::MakeRequestTimeout,
};
use rand::{rngs::SmallRng, SeedableRng};
use std::{net::SocketAddr, str::FromStr, sync::Arc};
use structopt::StructOpt;
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{RwLock, Semaphore},
    time::Duration,
};
use tokio_compat_02::FutureExt;
use tracing::debug_span;
use tracing_futures::Instrument;

#[async_trait::async_trait]
pub trait MakeClient<T> {
    type Client: Client;

    async fn make_client(&mut self, target: T) -> Self::Client;
}

#[async_trait::async_trait]
pub trait Client: Clone + Send + 'static {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status>;
}

#[derive(StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {
    #[structopt(long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(long)]
    requests_per_target: Optin<usize>,

    #[structopt(long, default_value = "0")]
    request_limit: usize,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "1s")]
    request_limit_window: Duration,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "10s")]
    request_timeout: Duration,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "10s")]
    connect_timeout: Duration,

    #[structopt(long)]
    total_requests: Option<usize>,

    #[structopt(long)]
    http_close: bool,

    #[structopt(long)]
    concurrency_limit: Option<usize>,

    #[structopt(long, default_value = "100=0")]
    response_sizes: Distribution,

    target: Target,

    targets: Vec<Target>,
}

type Target = Flavor<::http::Uri, ::http::Uri>;

#[derive(Clone, Debug)]
pub enum Flavor<H, G> {
    Http(H),
    Grpc(G),
}

// === impl Load ===C

impl Load {
    pub async fn run(self, threads: usize) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let Self {
            admin_addr,
            requests_per_target,
            connect_timeout,
            request_timeout,
            http_close,
            concurrency_limit,
            request_limit,
            request_limit_window,
            response_sizes,
            total_requests,
            target,
            targets,
        } = self;

        let histogram = Arc::new(RwLock::new(hdrhistogram::Histogram::new(3).unwrap()));
        let admin = Admin::new(histogram.clone());

        let concurrency_limit = Arc::new(Semaphore::new(
            concurrency_limit.filter(|c| *c > 0).unwrap_or(threads * 2),
        ));

        let rate_limit = RateLimit::spawn(request_limit, request_limit_window);
        let runner = Runner::new(
            requests_per_target.unwrap_or(0),
            total_requests.unwrap_or(0),
            (concurrency_limit, rate_limit),
            Arc::new(response_sizes),
            SmallRng::from_entropy(),
        );

        let connect = {
            let http = MakeHttp::new(connect_timeout, http_close);
            let grpc = MakeGrpc::new(connect_timeout, Duration::from_secs(1));
            let timeout = MakeRequestTimeout::new((http, grpc), request_timeout);
            MakeMetrics::new(timeout, histogram)
        };

        let targets = Some(target).into_iter().chain(targets).collect::<Vec<_>>();
        tokio::spawn(runner.run(connect, targets));

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

        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c() => {}
            _ = term.recv() => {}
        }

        Ok(())
    }
}

// === impl Flavor ===

#[async_trait::async_trait]
impl<H, G> MakeClient<Target> for (H, G)
where
    H: MakeClient<::http::Uri> + Send + Sync + 'static,
    H::Client: Send + Sync + 'static,
    G: MakeClient<::http::Uri> + Send + Sync + 'static,
    G::Client: Send + Sync + 'static,
{
    type Client = Flavor<H::Client, G::Client>;

    async fn make_client(&mut self, target: Target) -> Self::Client {
        match target {
            Target::Http(t) => {
                let c = self.0.make_client(t).await;
                Flavor::Http(c)
            }
            Target::Grpc(t) => {
                let c = self.1.make_client(t).await;
                Flavor::Grpc(c)
            }
        }
    }
}

#[async_trait::async_trait]
impl<H, G> Client for Flavor<H, G>
where
    H: Client + Send + Sync + 'static,
    G: Client + Send + Sync + 'static,
{
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        match self {
            Flavor::Http(h) => h.get(spec).await,
            Flavor::Grpc(g) => g.get(spec).await,
        }
    }
}

// === impl Target ===

impl FromStr for Target {
    type Err = Box<dyn std::error::Error + 'static>;

    fn from_str(s: &str) -> Result<Target, Self::Err> {
        let uri = ::http::Uri::from_str(s)?;
        match uri.scheme_str() {
            Some("grpc") | None => Ok(Target::Grpc(uri)),
            Some("http") => Ok(Target::Http(uri)),
            Some(scheme) => Err(UnsupportedScheme(scheme.to_string()).into()),
        }
    }
}

impl AsRef<::http::Uri> for Target {
    fn as_ref(&self) -> &::http::Uri {
        match self {
            Flavor::Http(h) => &h,
            Flavor::Grpc(g) => &g,
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
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
