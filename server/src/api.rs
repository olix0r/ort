use rand::{rngs::SmallRng, RngCore, SeedableRng};
use std::convert::TryInto;
use tokio::time::sleep;

mod proto {
    tonic::include_proto!("ortiofay.olix0r.net");
}

#[derive(Clone)]
pub(crate) struct Api {
    rng: SmallRng,
}

impl Api {
    pub fn server() -> proto::ortiofay_server::OrtiofayServer<Self> {
        proto::ortiofay_server::OrtiofayServer::new(Self {
            rng: SmallRng::from_entropy(),
        })
    }
}

#[tonic::async_trait]
impl proto::ortiofay_server::Ortiofay for Api {
    async fn get(
        &self,
        req: tonic::Request<proto::ResponseSpec>,
    ) -> Result<tonic::Response<proto::ResponseReply>, tonic::Status> {
        use proto::response_spec as spec;

        let proto::ResponseSpec {
            latency,
            result,
            data: _,
        } = req.into_inner();

        if let Some(l) = latency {
            if let Ok(l) = l.try_into() {
                sleep(l).await;
            }
        }

        match result {
            None => Ok(tonic::Response::new(proto::ResponseReply::default())),
            Some(spec::Result::Success(spec::Success { size })) => {
                let mut data = Vec::with_capacity(size.try_into().unwrap_or(0));
                self.rng.clone().fill_bytes(&mut data);
                Ok(tonic::Response::new(proto::ResponseReply { data }))
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
