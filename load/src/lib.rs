#![deny(warnings, rust_2018_idioms)]

mod rate_limit;
mod runner;

use self::{rate_limit::RateLimit, runner::Runner};
use std::{convert::Infallible, net::SocketAddr, time::Duration};
use structopt::StructOpt;
use tokio::{
    signal::unix::{signal, SignalKind},
    time::delay_for,
};
use tracing::{debug_span, warn};
use tracing_futures::Instrument;

mod proto {
    tonic::include_proto!("ortiofay.olix0r.net");
}

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {
    #[structopt(long, default_value = "0")]
    request_limit: usize,

    #[structopt(long, parse(try_from_str = parse_duration), default_value = "1s")]
    request_limit_window: Duration,

    #[structopt(short, long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(subcommand)]
    flavor: Flavor,
}

#[derive(Clone, Debug, StructOpt)]
#[structopt()]
enum Flavor {
    // // Generate HTTP/1.1 load
    // Http {},
    // Generate gRPC load
    Grpc {
        #[structopt(short, long, default_value = "1")]
        connections: usize,

        #[structopt(short, long, default_value = "1")]
        streams: usize,

        target: http::Uri,
    },
}

#[derive(Clone)]
struct MakeGrpc {
    target: http::Uri,
    backoff: Duration,
}

#[derive(Clone)]
struct Grpc(proto::ortiofay_client::OrtiofayClient<tonic::transport::Channel>);

#[async_trait::async_trait]
impl runner::MakeClient for MakeGrpc {
    type Client = Grpc;

    async fn make_client(&mut self) -> Grpc {
        loop {
            match proto::ortiofay_client::OrtiofayClient::connect(self.target.clone()).await {
                Ok(client) => return Grpc(client),
                Err(error) => {
                    warn!(%error, "Failed to connect");
                    delay_for(self.backoff).await;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl runner::Client for Grpc {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        let rsp = self.0.get(spec).await?;
        Ok(rsp.into_inner())
    }
}

impl Load {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        match self.flavor {
            Flavor::Grpc {
                connections,
                streams,
                target,
            } => {
                if connections == 0 || streams == 0 {
                    return Ok(());
                }

                let connect = MakeGrpc {
                    target,
                    backoff: Duration::from_secs(1),
                };

                let limit = RateLimit::new(self.request_limit, self.request_limit_window);
                Runner::new(connections, streams, limit).run(connect).await;

                let admin = Admin;
                let admin_addr = self.admin_addr;
                tokio::spawn(
                    async move {
                        admin
                            .serve(admin_addr)
                            .await
                            .expect("Admin server must not fail")
                    }
                    .instrument(debug_span!("admin")),
                );

                signal(SignalKind::terminate())?.recv().await;

                Ok(())
            }
        }
    }
}

#[derive(Clone)]
struct Admin;

impl Admin {
    async fn serve(&self, addr: SocketAddr) -> Result<(), hyper::Error> {
        let admin = self.clone();
        hyper::Server::bind(&addr)
            .serve(hyper::service::make_service_fn(move |_| {
                let admin = admin.clone();
                async move {
                    Ok::<_, Infallible>(hyper::service::service_fn(
                        move |req: http::Request<hyper::Body>| {
                            let admin = admin.clone();
                            async move { admin.handle(req).await }
                        },
                    ))
                }
            }))
            .await
    }

    async fn handle(
        &self,
        req: http::Request<hyper::Body>,
    ) -> Result<http::Response<hyper::Body>, Infallible> {
        if let "/live" | "/ready" = req.uri().path() {
            if let http::Method::GET | http::Method::HEAD = *req.method() {
                return Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(hyper::Body::default())
                    .unwrap());
            }
        }

        Ok(http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(hyper::Body::default())
            .unwrap())
    }
}

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
