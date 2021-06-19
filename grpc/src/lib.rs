#![deny(warnings, rust_2018_idioms)]

#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "server")]
pub mod server;

pub mod proto {
    tonic::include_proto!("ort.olix0r.net");
}
