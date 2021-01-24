//use ort_core::{Spec, Reply};
use crate::proto::{ort_server, response_spec as spec, ResponseReply, ResponseSpec};
use futures::prelude::*;
use linkerd_drain::Watch as Drain;
use ort_core::{Error, Ort, Reply, Spec};
use std::convert::TryInto;

#[derive(Clone)]
pub struct Server<O> {
    inner: O,
    window_size: u32,
}

impl<O: Ort + Sync> Server<O> {
    pub fn new(inner: O) -> Self {
        Self {
            inner,
            window_size: 2u32.pow(31) - 1,
        }
    }

    pub async fn serve(self, addr: std::net::SocketAddr, drain: Drain) -> Result<(), Error> {
        let (close, closed) = tokio::sync::oneshot::channel();

        tokio::pin! {
            let srv = tonic::transport::Server::builder()
                .initial_connection_window_size(self.window_size)
                .add_service(ort_server::OrtServer::new(self))
                .serve_with_shutdown(addr, closed.map(|_| ()));
        }
        tokio::select! {
            _ = (&mut srv) => {}
            handle = drain.signaled() => {
                let _ = close.send(());
                handle.release_after(srv).await?;
            }
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl<O: Ort + Sync> ort_server::Ort for Server<O> {
    async fn get(
        &self,
        req: tonic::Request<ResponseSpec>,
    ) -> Result<tonic::Response<ResponseReply>, tonic::Status> {
        let ResponseSpec {
            latency,
            result,
            data: _,
        } = req.into_inner();

        let latency = latency.and_then(|l| l.try_into().ok()).unwrap_or_default();

        let response_size = match result {
            None => 0,
            Some(spec::Result::Success(spec::Success { size })) => size as usize,
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
                return Err(tonic::Status::new(code, message));
            }
        };

        let spec = Spec {
            latency,
            response_size,
        };
        let mut inner = self.inner.clone();
        inner
            .ort(spec)
            .await
            .map(|Reply { data }| {
                tonic::Response::new(ResponseReply {
                    data: data.into_iter().collect(),
                })
            })
            .map_err(|e| tonic::Status::internal(e.to_string()))
    }
}
