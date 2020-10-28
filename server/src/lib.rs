#![deny(warnings, rust_2018_idioms)]

mod api;

use std::net::SocketAddr;
use structopt::StructOpt;
use tokio_compat_02::FutureExt;

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load target")]
pub struct Server {
    #[structopt(long, parse(try_from_str), default_value = "0.0.0.0:8079")]
    grpc_addr: SocketAddr,
    //#[structopt(long, parse(try_from_str), default_value = "0.0.0.0:8080")]
    //http_addr: SocketAddr,
}

impl Server {
    pub async fn run(self) -> Result<(), tonic::transport::Error> {
        tonic::transport::Server::builder()
            .add_service(api::Api::server())
            .serve(self.grpc_addr)
            .compat()
            .await
    }
}
