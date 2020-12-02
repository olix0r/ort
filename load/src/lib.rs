#![deny(warnings, rust_2018_idioms)]

mod admin;
mod limit;
mod metrics;
mod rate_limit;
mod runner;
mod timeout;

use self::{
    admin::Admin, metrics::MakeMetrics, rate_limit::RateLimit, runner::Runner,
    timeout::MakeRequestTimeout,
};
use ort_core::{
    latency, parse_duration, Distribution, Error, MakeOrt, MakeReconnect, Ort, Reply, Spec,
};
use ort_grpc::client::MakeGrpc;
use ort_http::client::MakeHttp;
use ort_tcp::client::MakeTcp;
use std::{net::SocketAddr, str::FromStr, sync::Arc};
use structopt::StructOpt;
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    time::Duration,
};
use tracing::debug_span;
use tracing_futures::Instrument;

#[derive(StructOpt)]
#[structopt(name = "load", about = "Load generator")]
pub struct Cmd {
    #[structopt(long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

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
    requests_per_client: Option<usize>,

    #[structopt(long, default_value = "0")]
    response_latency: latency::Distribution,

    #[structopt(long, default_value = "0")]
    response_size: Distribution,

    target: Target,
}

type Target = Flavor<hyper::Uri, hyper::Uri, String>;

#[derive(Clone, Debug)]
pub enum Flavor<H, G, T> {
    Http(H),
    Grpc(G),
    Tcp(T),
}

// === impl Load ===

impl Cmd {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let Self {
            admin_addr,
            connect_timeout,
            request_timeout,
            request_limit,
            request_limit_window,
            response_latency,
            response_size,
            total_requests,
            requests_per_client,
            target,
        } = self;

        let limit = RateLimit::spawn(request_limit, request_limit_window);
        let runner = Runner::new(
            total_requests,
            requests_per_client,
            limit,
            Arc::new(response_latency),
            Arc::new(response_size),
        );

        let (connect, report) = {
            let http = MakeHttp::new(connect_timeout);
            let grpc = MakeGrpc::new();
            let tcp = MakeTcp::new(100_000);
            let connect = (http, grpc, tcp);
            let timeout = MakeRequestTimeout::new(connect, request_timeout);
            let (metrics, report) = MakeMetrics::new(timeout);
            (
                MakeReconnect::new(metrics, connect_timeout, Duration::from_secs(1)),
                report,
            )
        };

        let admin = Admin::new(report);

        tokio::spawn(runner.run(connect, target));

        tokio::spawn(
            async move {
                admin
                    .serve(admin_addr)
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
impl<H, G, T> MakeOrt<Target> for (H, G, T)
where
    H: MakeOrt<hyper::Uri> + Send + Sync + 'static,
    H::Ort: Send + Sync + 'static,
    G: MakeOrt<hyper::Uri> + Send + Sync + 'static,
    G::Ort: Send + Sync + 'static,
    T: MakeOrt<String> + Send + Sync + 'static,
    T::Ort: Send + Sync + 'static,
{
    type Ort = Flavor<H::Ort, G::Ort, T::Ort>;

    async fn make_ort(&mut self, target: Target) -> Result<Self::Ort, Error> {
        match target {
            Target::Http(t) => {
                let c = self.0.make_ort(t).await?;
                Ok(Flavor::Http(c))
            }
            Target::Grpc(t) => {
                let c = self.1.make_ort(t).await?;
                Ok(Flavor::Grpc(c))
            }
            Target::Tcp(t) => {
                let c = self.2.make_ort(t).await?;
                Ok(Flavor::Tcp(c))
            }
        }
    }
}

#[async_trait::async_trait]
impl<H, G, T> Ort for Flavor<H, G, T>
where
    H: Ort + Send + Sync + 'static,
    G: Ort + Send + Sync + 'static,
    T: Ort + Send + Sync + 'static,
{
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        match self {
            Flavor::Http(h) => h.ort(spec).await,
            Flavor::Grpc(g) => g.ort(spec).await,
            Flavor::Tcp(t) => t.ort(spec).await,
        }
    }
}

// === impl Target ===

impl Target {
    fn uri_default_port(
        uri: http::Uri,
        port: u16,
    ) -> Result<http::Uri, Box<dyn std::error::Error + 'static>> {
        let mut p = uri.into_parts();
        p.authority = match p.authority {
            Some(a) => {
                let s = format!("{}:{}", a.host(), a.port_u16().unwrap_or(port));
                Some(http::uri::Authority::from_str(&s)?)
            }
            None => return Err(InvalidTarget(http::Uri::from_parts(p)?).into()),
        };
        let uri = http::Uri::from_parts(p)?;
        Ok(uri)
    }
}

impl FromStr for Target {
    type Err = Box<dyn std::error::Error + 'static>;

    fn from_str(s: &str) -> Result<Target, Self::Err> {
        let uri = http::Uri::from_str(s)?;
        match uri.clone().scheme_str() {
            Some("grpc") => {
                let uri = Self::uri_default_port(uri, 8070)?;
                Ok(Target::Grpc(uri))
            }
            Some("http") => {
                let uri = Self::uri_default_port(uri, 8080)?;
                Ok(Target::Http(uri))
            }
            Some("tcp") => {
                let uri = Self::uri_default_port(uri, 8090)?;
                match uri.authority() {
                    Some(a) => Ok(Target::Tcp(a.to_string())),
                    None => return Err(InvalidTarget(uri).into()),
                }
            }
            _ => Err(InvalidTarget(uri).into()),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flavor::Http(h) => h.fmt(f),
            Flavor::Grpc(g) => g.fmt(f),
            Flavor::Tcp(t) => t.fmt(f),
        }
    }
}

#[derive(Debug)]
struct InvalidTarget(hyper::Uri);

impl std::fmt::Display for InvalidTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unsupported scheme: {}", self.0)
    }
}

impl std::error::Error for InvalidTarget {}
