use crate::proto;
use std::time::Duration;
use tokio::time::sleep;
use tokio_compat_02::FutureExt;
use tracing::warn;

#[derive(Clone)]
pub struct MakeGrpc {
    backoff: Duration,
}

#[derive(Clone)]
pub struct Grpc(proto::ort_client::OrtClient<tonic::transport::Channel>);

impl MakeGrpc {
    pub fn new(backoff: Duration) -> Self {
        Self { backoff }
    }
}

#[async_trait::async_trait]
impl crate::MakeClient<http::Uri> for MakeGrpc {
    type Client = Grpc;

    async fn make_client(&mut self, target: http::Uri) -> Grpc {
        loop {
            match proto::ort_client::OrtClient::connect(target.clone())
                .compat()
                .await
            {
                Ok(client) => return Grpc(client),
                Err(error) => {
                    warn!(%error, "Failed to connect");
                    sleep(self.backoff).await;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl crate::Client for Grpc {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        let rsp = self.0.get(spec).compat().await?;
        Ok(rsp.into_inner())
    }
}
