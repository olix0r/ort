#![deny(warnings, rust_2018_idioms)]

pub mod client;
pub mod server;

mod proto {
    tonic::include_proto!("ort.olix0r.net");
}
