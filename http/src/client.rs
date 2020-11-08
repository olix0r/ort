use crate::proto::{self, response_spec as spec};
use std::convert::TryFrom;
use tokio::time::Duration;
use tokio_compat_02::FutureExt;

#[derive(Clone)]
pub struct MakeHttp {
    close: bool,
    connect_timeout: Duration,
}

#[derive(Clone)]
pub struct Http {
    client: hyper::Client<hyper::client::HttpConnector>,
    target: http::Uri,
    close: bool,
}

impl MakeHttp {
    pub fn new(connect_timeout: Duration, close: bool) -> Self {
        Self {
            connect_timeout,
            close,
        }
    }
}

#[async_trait::async_trait]
impl crate::MakeClient<http::Uri> for MakeHttp {
    type Client = Http;

    async fn make_client(&mut self, target: http::Uri) -> Http {
        let mut connect = hyper::client::HttpConnector::new();
        connect.set_connect_timeout(Some(self.connect_timeout));
        connect.set_nodelay(true);
        connect.set_reuse_address(true);
        Http {
            target,
            client: hyper::Client::builder().build(connect),
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

        uri = {
            let latency_ms = spec
                .latency
                .and_then(|l| Duration::try_from(l).ok())
                .unwrap_or_default()
                .as_millis() as i64;

            let size = match spec.result {
                Some(spec::Result::Success(spec::Success { size })) => size,
                _ => 0,
            };

            tracing::trace!(latency_ms, size);
            uri.path_and_query(
                http::uri::PathAndQuery::try_from(
                    format!("/?latency_ms={}&size={}", latency_ms, size).as_str(),
                )
                .expect("query must be valid"),
            )
        };

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
        if !rsp.status().is_success() {
            return Err(tonic::Status::internal(format!(
                "Unexpected response status: {}",
                rsp.status()
            )));
        }
        let data = hyper::body::to_bytes(rsp.into_body())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .into_iter()
            .collect::<Vec<u8>>();
        Ok(proto::ResponseReply { data })
    }
}
