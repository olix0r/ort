#![deny(warnings, rust_2018_idioms)]

mod grpc;
mod http;

use rand::{rngs::SmallRng, SeedableRng};
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio::signal::{
    ctrl_c,
    unix::{signal, SignalKind},
};
use tokio_compat_02::FutureExt;

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load target")]
pub struct Server {
    #[structopt(short, long, default_value = "0.0.0.0:8070")]
    grpc_addr: SocketAddr,

    #[structopt(short, long, default_value = "0.0.0.0:8080")]
    http_addr: SocketAddr,

    // #[structopt(short, long, default_value = "0.0.0.0:8090")]
    // tcp_addr: SocketAddr,
}

impl Server {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let rng = SmallRng::from_entropy();

        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(grpc::Server::new(rng.clone()))
                .serve(self.grpc_addr)
                .compat(),
        );

        tokio::spawn(http::serve(self.http_addr, rng).compat());

        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c() => {}
            _ = term.recv() => {}
        }

        Ok(())
    }
}
