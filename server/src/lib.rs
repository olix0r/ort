#![deny(warnings, rust_2018_idioms)]

mod replier;

use self::replier::Replier;
use clap::Parser;
use ort_core::latency;
use ort_grpc::server as grpc;
use ort_http::server as http;
use ort_tcp::server as tcp;
use std::net::SocketAddr;
use tokio::signal::{
    ctrl_c,
    unix::{signal, SignalKind},
};
use tracing::{debug, info_span, instrument, Instrument};

#[derive(Parser)]
#[clap(name = "server", about = "Load target")]
pub struct Cmd {
    #[clap(short, long, default_value = "0.0.0.0:9090")]
    admin_addr: SocketAddr,

    #[clap(short, long, default_value = "0.0.0.0:8070")]
    grpc_addr: SocketAddr,

    #[clap(short, long, default_value = "0.0.0.0:8080")]
    http_addr: SocketAddr,

    #[clap(short, long, default_value = "0.0.0.0:8090")]
    tcp_addr: SocketAddr,

    #[clap(long, default_value = "0")]
    response_latency: latency::Distribution,
}

impl Cmd {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let replier = Replier::new(self.response_latency);

        let (close, closed) = drain::channel();
        tokio::spawn(
            grpc::Server::new(replier.clone())
                .serve(self.grpc_addr, closed.clone())
                .instrument(info_span!("grpc")),
        );
        tokio::spawn(
            http::Server::new(replier.clone())
                .serve(self.http_addr, closed.clone())
                .instrument(info_span!("http")),
        );
        tokio::spawn(
            tcp::Server::new(replier)
                .serve(self.tcp_addr, closed)
                .instrument(info_span!("tcp")),
        );

        tokio::spawn(admin(self.admin_addr));

        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c() => {}
            _ = term.recv() => {}
        }

        close.drain().await;

        Ok(())
    }
}

/// Runs an admin server that can be used for health checking.
///
/// It always returns a successful response with a body of `ok\n`.
#[instrument]
async fn admin(addr: SocketAddr) -> Result<(), hyper::Error> {
    use std::convert::Infallible;

    hyper::Server::bind(&addr)
        .serve(hyper::service::make_service_fn(move |_| async {
            Ok::<_, Infallible>(hyper::service::service_fn(
                |req: hyper::Request<hyper::Body>| async move {
                    debug!(?req);
                    Ok::<_, Infallible>(match req.uri().path() {
                        "/ready" | "/live" => hyper::Response::new(hyper::Body::from("ok\n")),
                        _ => hyper::Response::builder()
                            .status(hyper::StatusCode::NOT_FOUND)
                            .body(hyper::Body::from("ok\n"))
                            .unwrap(),
                    })
                },
            ))
        }))
        .await
}
