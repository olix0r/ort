#![deny(warnings, rust_2018_idioms)]

use structopt::StructOpt;
use std::net::SocketAddr;

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {
    #[structopt(short, long, default_value = "10")]
    requests_per_second: usize,

    #[structopt(short, long, default_value = "10000")]
    total_requests: usize,

    #[structopt(short, long, default_value = "1")]
    concurrency: usize,

    #[structopt(short, long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(subcommand)]
    flavor: Flavor,
}

#[derive(Clone, Debug, StructOpt)]
#[structopt()]
pub enum Flavor {
    // Generate HTTP/1.1 load
    Http {},
    // // Generate gRPC load
    // Grpc {}
}

impl Load {
    pub async fn run(self) {
        match self.flavor {
            Http
        }
    }
}
