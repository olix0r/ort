use crate::proto;
use std::time::Duration;
use tokio::time::sleep;
use tokio_compat_02::FutureExt;
use tracing::warn;

#[derive(Clone)]
pub struct MakeGrpc {
    target: http::Uri,
    backoff: Duration,
}

#[derive(Clone)]
pub struct Grpc(proto::ortiofay_client::OrtiofayClient<tonic::transport::Channel>);

impl MakeGrpc {
    pub fn new(target: http::Uri, backoff: Duration) -> Self {
        Self { target, backoff }
    }
}

#[async_trait::async_trait]
impl crate::MakeClient for MakeGrpc {
    type Client = Grpc;

    async fn make_client(&mut self) -> Grpc {
        loop {
            match proto::ortiofay_client::OrtiofayClient::connect(self.target.clone())
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
