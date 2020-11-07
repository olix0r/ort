use crate::proto::{ort_client, response_spec as spec, ResponseSpec};
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use tokio::time::{sleep, timeout, Duration};
use tokio_compat_02::FutureExt;
use tracing::warn;

#[derive(Clone)]
pub struct MakeGrpc {
    connect_timeout: Duration,
    backoff: Duration,
}

#[derive(Clone)]
pub struct Grpc(ort_client::OrtClient<tonic::transport::Channel>);

impl MakeGrpc {
    pub fn new(connect_timeout: Duration, backoff: Duration) -> Self {
        Self {
            connect_timeout,
            backoff,
        }
    }
}

#[async_trait::async_trait]
impl MakeOrt<http::Uri> for MakeGrpc {
    type Ort = Grpc;

    async fn make_ort(&mut self, target: http::Uri) -> Result<Grpc, Error> {
        loop {
            let connect = ort_client::OrtClient::connect(target.clone()).compat();
            match timeout(self.connect_timeout, connect).await {
                Ok(Ok(client)) => return Ok(Grpc(client)),
                Ok(Err(error)) => {
                    warn!(%error, "Failed to connect");
                    sleep(self.backoff).await;
                }
                Err(_) => {
                    warn!("Connection timed out");
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Ort for Grpc {
    async fn ort(
        &mut self,
        Spec {
            latency,
            response_size,
            data,
        }: Spec,
    ) -> Result<Reply, Error> {
        let req = ResponseSpec {
            latency: Some(latency.into()),
            result: Some(spec::Result::Success(spec::Success {
                size: response_size as i64,
            })),
            data: data.into_iter().collect(),
        };

        let rsp = self.0.get(req).compat().await?.into_inner();
        Ok(Reply {
            data: rsp.data.into(),
        })
    }
}
