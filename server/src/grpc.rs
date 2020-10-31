mod proto {
    tonic::include_proto!("ortiofay.olix0r.net");
}

pub use self::proto::{ortiofay_server::Ortiofay, response_spec, ResponseReply, ResponseSpec};
use rand::{rngs::SmallRng, RngCore};
use std::convert::TryInto;
use tokio::time;

#[derive(Clone)]
pub(crate) struct Server {
    rng: SmallRng,
}

impl Server {
    pub fn new(rng: SmallRng) -> proto::ortiofay_server::OrtiofayServer<Self> {
        proto::ortiofay_server::OrtiofayServer::new(Self { rng })
    }
}

#[tonic::async_trait]
impl Ortiofay for Server {
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
                time::sleep(l).await;
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
