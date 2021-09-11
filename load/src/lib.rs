#![deny(warnings, rust_2018_idioms)]

mod admin;
mod concurrency_ramp;
mod metrics;
mod rate_limit;
mod runner;
mod timeout;

use crate::concurrency_ramp::ConcurrencyRamp;

use self::{
    admin::Admin, metrics::MakeMetrics, rate_limit::RateLimit, runner::Runner,
    timeout::MakeRequestTimeout,
};
use anyhow::{anyhow, bail, Result};
use ort_core::{latency, parse_duration, Distribution, Error, MakeOrt, Ort, Reply, Spec};
use ort_grpc::client::MakeGrpc;
use ort_http::client::MakeHttp;
use ort_tcp::client::MakeTcp;
use std::{fmt::Debug, net::SocketAddr, str::FromStr, sync::Arc};
use structopt::StructOpt;
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    time::Duration,
};
use tracing::{debug_span, Instrument};

#[derive(StructOpt)]
#[structopt(name = "load", about = "Load generator")]
pub struct Cmd {
    #[structopt(long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(long)]
    clients: Option<usize>,

    #[structopt(long)]
    concurrency_limit_init: Option<usize>,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "0s")]
    concurrency_limit_ramp_period: Duration,

    #[structopt(long)]
    concurrency_limit: Option<usize>,

    #[structopt(long)]
    request_limit_init: Option<usize>,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "0s")]
    request_limit_ramp_period: Duration,

    #[structopt(long, default_value = "0")]
    request_limit: usize,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "1s")]
    request_limit_window: Duration,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "10s")]
    request_timeout: Duration,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "1s")]
    connect_timeout: Duration,

    #[structopt(long)]
    total_requests: Option<usize>,

    #[structopt(long, default_value = "0")]
    response_latency: latency::Distribution,

    #[structopt(long, default_value = "0")]
    response_size: Distribution,

    target: Target,
}

#[derive(Copy, Clone, Debug)]
struct Ramp {
    min: usize,
    max: usize,
    period: Duration,
}

type Target = Flavor<hyper::Uri, hyper::Uri, String>;

#[derive(Clone, Debug)]
pub enum Flavor<H, G, T> {
    Http(H),
    Grpc(G),
    Tcp(T),
}

// === impl Cmd ===

impl Cmd {
    pub async fn run(self, threads: usize) -> Result<()> {
        let Self {
            admin_addr,
            clients,
            connect_timeout,
            concurrency_limit_init,
            concurrency_limit,
            concurrency_limit_ramp_period,
            request_timeout,
            request_limit_init,
            request_limit,
            request_limit_ramp_period,
            request_limit_window,
            response_latency,
            response_size,
            total_requests,
            target,
        } = self;

        let concurrency = if let Some(c) = concurrency_limit {
            let ramp = Ramp::try_new(
                concurrency_limit_init.unwrap_or(c),
                c,
                concurrency_limit_ramp_period,
            )?;
            Some(ConcurrencyRamp::spawn(ramp))
        } else {
            None
        };

        let rate_limit = RateLimit::spawn(
            Ramp::try_new(
                request_limit_init.unwrap_or(request_limit),
                request_limit,
                request_limit_ramp_period,
            )?,
            request_limit_window,
        );

        let runner = Runner::new(
            clients.unwrap_or(threads),
            total_requests,
            (concurrency, rate_limit),
            Arc::new(response_latency),
            Arc::new(response_size),
        );

        let (connect, report) = {
            let client = (
                MakeHttp::new(concurrency_limit, connect_timeout),
                MakeGrpc::default(),
                MakeTcp::new(100_000),
            );
            let client = MakeRequestTimeout::new(client, request_timeout);
            MakeMetrics::new(client)
        };

        tokio::spawn(
            async move {
                Admin::new(report)
                    .serve(admin_addr)
                    .await
                    .expect("Admin server must not fail")
            }
            .instrument(debug_span!("admin")),
        );

        tokio::spawn(runner.run(connect, target));

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
    fn uri_default_port(uri: http::Uri, port: u16) -> Result<http::Uri> {
        let mut p = uri.into_parts();
        p.authority = match p.authority {
            Some(a) => {
                let s = format!("{}:{}", a.host(), a.port_u16().unwrap_or(port));
                Some(http::uri::Authority::from_str(&s)?)
            }
            None => bail!("missing authority: {}", http::Uri::from_parts(p)?),
        };
        let uri = http::Uri::from_parts(p)?;
        Ok(uri)
    }
}

impl FromStr for Target {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Target> {
        let uri = http::Uri::from_str(s)?;
        match uri.scheme_str() {
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
                let a = uri
                    .authority()
                    .ok_or_else(|| anyhow!("missing authority"))?;
                Ok(Target::Tcp(a.to_string()))
            }
            Some(s) => bail!("invalid scheme: {}", s),
            None => bail!("missing scheme"),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flavor::Http(h) => std::fmt::Display::fmt(h, f),
            Flavor::Grpc(g) => std::fmt::Display::fmt(g, f),
            Flavor::Tcp(t) => std::fmt::Display::fmt(t, f),
        }
    }
}

// === impl Ramp ===

impl From<usize> for Ramp {
    fn from(value: usize) -> Self {
        Self {
            min: value,
            max: value,
            period: Duration::from_secs(0),
        }
    }
}

impl Ramp {
    pub fn try_new(min: usize, max: usize, period: Duration) -> Result<Self> {
        if min > max {
            bail!("min must be <= max");
        }
        if period.as_secs() == 0 && min != max {
            bail!("period must be set if min != max")
        }
        Ok(Self { min, max, period })
    }

    fn init(&self) -> usize {
        if self.period.as_secs() == 0 {
            self.max
        } else {
            self.min
        }
    }
}
