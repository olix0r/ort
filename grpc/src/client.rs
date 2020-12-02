use crate::proto::{ort_client, response_spec as spec, ResponseSpec};
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use tracing::trace;

#[derive(Clone)]
pub struct MakeGrpc {
    window_size: u32,
}

#[derive(Clone)]
pub struct Grpc(ort_client::OrtClient<tonic::transport::Channel>);

impl Default for MakeGrpc {
    fn default() -> Self {
        Self {
            window_size: 2u32.pow(31) - 1,
        }
    }
}

#[async_trait::async_trait]
impl MakeOrt<http::Uri> for MakeGrpc {
    type Ort = Grpc;

    async fn make_ort(&mut self, target: http::Uri) -> Result<Grpc, Error> {
        let chan = tonic::transport::Channel::builder(target)
            .initial_connection_window_size(self.window_size);
        let c = ort_client::OrtClient::connect(chan).await?;
        Ok(Grpc(c))
    }
}

#[async_trait::async_trait]
impl Ort for Grpc {
    async fn ort(
        &mut self,
        Spec {
            latency,
            response_size,
        }: Spec,
    ) -> Result<Reply, Error> {
        let req = ResponseSpec {
            latency: Some(latency.into()),
            result: Some(spec::Result::Success(spec::Success {
                size: response_size as i64,
            })),
            ..ResponseSpec::default()
        };

        trace!("Issuing request");
        let res = self.0.get(req).await;
        trace!("Received response");
        let rsp = res?.into_inner();
        Ok(Reply {
            data: rsp.data.into(),
        })
    }
}
