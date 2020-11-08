//use ort_core::{Spec, Reply};
use crate::proto::{ort_server, response_spec as spec, ResponseReply, ResponseSpec};
use rand::{rngs::SmallRng, RngCore};
use std::convert::TryInto;
use tokio_02::time;

#[derive(Clone)]
pub struct Server {
    rng: SmallRng,
}

impl Server {
    pub fn new(rng: SmallRng) -> ort_server::OrtServer<Self> {
        ort_server::OrtServer::new(Self { rng })
    }
}

#[tonic::async_trait]
impl ort_server::Ort for Server {
    async fn get(
        &self,
        req: tonic::Request<ResponseSpec>,
    ) -> Result<tonic::Response<ResponseReply>, tonic::Status> {
        let ResponseSpec {
            latency,
            result,
            data: _,
        } = req.into_inner();

        let l = latency
            .and_then(|l| l.try_into().ok())
            .unwrap_or(time::Duration::from_secs(0));
        time::delay_for(l).await;

        match result {
            None => Ok(tonic::Response::new(ResponseReply::default())),
            Some(spec::Result::Success(spec::Success { size })) => {
                let mut data = Vec::with_capacity(size.try_into().unwrap_or(0));
                self.rng.clone().fill_bytes(&mut data);
                Ok(tonic::Response::new(ResponseReply { data }))
            }
            Some(spec::Result::Error(spec::Error { code, message })) => {
                let code = match code {
                    1 => tonic::Code::Cancelled,
                    2 => tonic::Code::Unknown,
                    3 => tonic::Code::InvalidArgument,
                    4 => tonic::Code::DeadlineExceeded,
                    5 => tonic::Code::NotFound,
                    6 => tonic::Code::AlreadyExists,
                    7 => tonic::Code::PermissionDenied,
                    8 => tonic::Code::ResourceExhausted,
                    9 => tonic::Code::FailedPrecondition,
                    10 => tonic::Code::Aborted,
                    11 => tonic::Code::OutOfRange,
                    12 => tonic::Code::Unimplemented,
                    13 => tonic::Code::Internal,
                    14 => tonic::Code::Unavailable,
                    15 => tonic::Code::DataLoss,
                    16 => tonic::Code::Unauthenticated,
                    _ => tonic::Code::InvalidArgument,
                };
                Err(tonic::Status::new(code, message))
            }
        }
    }
}
