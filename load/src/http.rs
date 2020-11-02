use crate::proto::{self, response_spec as spec};
use std::convert::TryFrom;
use tokio_compat_02::FutureExt;

#[derive(Clone)]
pub struct MakeHttp {
    target: http::Uri,
    close: bool,
}

#[derive(Clone)]
pub struct Http {
    client: hyper::Client<hyper::client::HttpConnector>,
    target: http::Uri,
    close: bool,
}

impl MakeHttp {
    pub fn new(target: http::Uri, close: bool) -> Self {
        Self { target, close }
    }
}

#[async_trait::async_trait]
impl crate::MakeClient for MakeHttp {
    type Client = Http;

    async fn make_client(&mut self) -> Http {
        Http {
            target: self.target.clone(),
            client: hyper::Client::new(),
            close: self.close,
        }
    }
}

#[async_trait::async_trait]
impl crate::Client for Http {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        let mut uri = http::Uri::builder();
        if let Some(s) = self.target.scheme() {
            uri = uri.scheme(s.clone());
        }
        if let Some(a) = self.target.authority() {
            uri = uri.authority(a.clone());
        }
        let latency_ms = if let Some(l) = spec.latency {
            (l.seconds.saturating_mul(1000) + l.nanos as i64).max(0)
        } else {
            0
        };
        let size = match spec.result {
            Some(spec::Result::Success(spec::Success { size })) => size,
            _ => 0,
        };
        uri = uri.path_and_query(
            http::uri::PathAndQuery::try_from(
                format!("/?latency_ms={}&size={}", latency_ms, size).as_str(),
            )
            .unwrap(),
        );

        let mut req = http::Request::builder();
        if self.close {
            req = req.header(http::header::CONNECTION, "close");
        }

        let rsp = self
            .client
            .request(
                req.uri(uri.build().unwrap())
                    .body(hyper::Body::default())
                    .unwrap(),
            )
            .compat()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        if rsp.status() != http::StatusCode::OK {
            return Err(tonic::Status::internal("Non-200 response received"));
        }
        let data = hyper::body::to_bytes(rsp.into_body())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .into_iter()
            .collect::<Vec<u8>>();
        Ok(proto::ResponseReply { data })
    }
}
