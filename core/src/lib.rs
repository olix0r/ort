#![deny(warnings, rust_2018_idioms)]

use bytes::Bytes;
use std::time::Duration;

#[async_trait::async_trait]
pub trait MakeOrt<T>: Clone + Send + 'static {
    type Ort: Ort;

    async fn make_ort(&mut self, target: T) -> Result<Self::Ort, Error>;
}

#[async_trait::async_trait]
pub trait Ort: Clone + Send + 'static {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error>;
}

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub latency: Duration,
    pub response_size: usize,
    pub data: Bytes,
}

#[derive(Clone, Debug, Default)]
pub struct Reply {
    pub data: Bytes,
}
