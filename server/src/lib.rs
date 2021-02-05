#![deny(warnings, rust_2018_idioms)]

mod replier;

use self::replier::Replier;
use linkerd_drain as drain;
use ort_core::latency;
use ort_grpc::server as grpc;
use ort_http::server as http;
use ort_tcp::server as tcp;
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio::signal::{
    ctrl_c,
    unix::{signal, SignalKind},
};

#[derive(StructOpt)]
#[structopt(name = "server", about = "Load target")]
pub struct Cmd {
    #[structopt(short, long, default_value = "0.0.0.0:8070")]
    grpc_addr: SocketAddr,

    #[structopt(short, long, default_value = "0.0.0.0:8080")]
    http_addr: SocketAddr,

    #[structopt(long, default_value = "0")]
    response_latency: latency::Distribution,

    #[structopt(short, long, default_value = "0.0.0.0:8090")]
    tcp_addr: SocketAddr,

    /// Configures the HTTP/2 `SETTINGS_MAX_CONCURRENT_STREAMS` option sent to
    /// the remote by the gRPC server.
    ///
    /// This negotiates the maximum number of concurrent HTTP/2 streams that may
    /// be initiated by the *client* on this connection.
    #[structopt(short, long)]
    max_concurrent_streams: Option<u32>,
}

impl Cmd {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let replier = Replier::new(self.response_latency);

        let (close, closed) = drain::channel();
        tokio::spawn(
            grpc::Server::new(replier.clone())
                .max_concurrent_streams(self.max_concurrent_streams)
                .serve(self.grpc_addr, closed.clone()),
        );
        tokio::spawn(http::Server::new(replier.clone()).serve(self.http_addr, closed.clone()));
        tokio::spawn(tcp::Server::new(replier).serve(self.tcp_addr, closed));

        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c() => {}
            _ = term.recv() => {}
        }

        close.drain().await;

        Ok(())
    }
}
