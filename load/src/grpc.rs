use crate::proto;
use tokio::time::{sleep, timeout, Duration};
use tokio_compat_02::FutureExt;
use tracing::warn;

#[derive(Clone)]
pub struct MakeGrpc {
    connect_timeout: Duration,
    backoff: Duration,
}

#[derive(Clone)]
pub struct Grpc(proto::ort_client::OrtClient<tonic::transport::Channel>);

impl MakeGrpc {
    pub fn new(connect_timeout: Duration, backoff: Duration) -> Self {
        Self {
            connect_timeout,
            backoff,
        }
    }
}

#[async_trait::async_trait]
impl crate::MakeClient<http::Uri> for MakeGrpc {
    type Client = Grpc;

    async fn make_client(&mut self, target: http::Uri) -> Grpc {
        loop {
            let connect = proto::ort_client::OrtClient::connect(target.clone()).compat();
            match timeout(self.connect_timeout, connect).await {
                Ok(Ok(client)) => return Grpc(client),
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
impl crate::Client for Grpc {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        let rsp = self.0.get(spec).compat().await?;
        Ok(rsp.into_inner())
    }
}
