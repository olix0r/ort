#![deny(warnings, rust_2018_idioms)]

mod replier;

use self::replier::Replier;
use ort_grpc::server as grpc;
use ort_http::server as http;
use ort_tcp::server as tcp;
use rand::{rngs::SmallRng, SeedableRng};
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio::signal::{
    ctrl_c,
    unix::{signal, SignalKind},
};

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "server", about = "Load target")]
pub struct Cmd {
    #[structopt(short, long, default_value = "0.0.0.0:8070")]
    grpc_addr: SocketAddr,

    #[structopt(short, long, default_value = "0.0.0.0:8080")]
    http_addr: SocketAddr,

    #[structopt(short, long, default_value = "0.0.0.0:8090")]
    tcp_addr: SocketAddr,
}

impl Cmd {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let rng = SmallRng::from_entropy();
        let replier = Replier::new(rng);

        tokio::spawn(grpc::Server::new(replier.clone()).serve(self.grpc_addr));
        tokio::spawn(http::Server::new(replier.clone()).serve(self.http_addr));
        tokio::spawn(tcp::Server::new(replier).serve(self.tcp_addr));

        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c() => {}
            _ = term.recv() => {}
        }

        Ok(())
    }
}
