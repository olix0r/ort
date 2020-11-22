use crate::proto::{ort_client, response_spec as spec, ResponseSpec};
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};

#[derive(Clone)]
pub struct MakeGrpc {}

#[derive(Clone)]
pub struct Grpc(ort_client::OrtClient<tonic::transport::Channel>);

impl MakeGrpc {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl MakeOrt<http::Uri> for MakeGrpc {
    type Ort = Grpc;

    async fn make_ort(&mut self, target: http::Uri) -> Result<Grpc, Error> {
        let c = ort_client::OrtClient::connect(target.clone()).await?;
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

        let rsp = self.0.get(req).await?.into_inner();
        Ok(Reply {
            data: rsp.data.into(),
        })
    }
}
