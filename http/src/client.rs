use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
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
impl MakeOrt<http::Uri> for MakeHttp {
    type Ort = Http;

    async fn make_ort(&mut self, target: http::Uri) -> Result<Http, Error> {
        let mut connect = hyper::client::HttpConnector::new();
        connect.set_connect_timeout(Some(self.connect_timeout));
        connect.set_nodelay(true);
        connect.set_reuse_address(true);
        Ok(Http {
            target,
            client: hyper::Client::builder().build(connect),
            close: self.close,
        })
    }
}

#[async_trait::async_trait]
impl Ort for Http {
    async fn ort(
        &mut self,
        Spec {
            latency,
            response_size,
            data: _,
        }: Spec,
    ) -> Result<Reply, Error> {
        let mut uri = http::Uri::builder();
        if let Some(s) = self.target.scheme() {
            uri = uri.scheme(s.clone());
        }

        if let Some(a) = self.target.authority() {
            uri = uri.authority(a.clone());
        }

        uri = {
            let latency_ms = Duration::try_from(latency).unwrap_or_default().as_millis() as i64;

            tracing::trace!(latency_ms, response_size);
            uri.path_and_query(
                http::uri::PathAndQuery::try_from(
                    format!("/?latency_ms={}&size={}", latency_ms, response_size).as_str(),
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
            .await?;

        if !rsp.status().is_success() {
            unimplemented!()
        }

        let data = hyper::body::to_bytes(rsp.into_body()).await?;

        Ok(Reply { data })
    }
}
