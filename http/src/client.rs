use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use std::convert::TryFrom;
use tokio::time::Duration;

#[derive(Clone)]
pub struct MakeHttp {
    concurrency: Option<usize>,
    connect_timeout: Duration,
}

#[derive(Clone)]
pub struct Http {
    client: hyper::Client<hyper::client::HttpConnector>,
    target: http::Uri,
}

impl MakeHttp {
    pub fn new(concurrency: Option<usize>, connect_timeout: Duration) -> Self {
        Self {
            concurrency,
            connect_timeout,
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

        let mut builder = hyper::Client::builder();
        if let Some(c) = self.concurrency {
            builder.pool_max_idle_per_host(c);
        }
        let client = builder.build(connect);

        Ok(Http { target, client })
    }
}

#[async_trait::async_trait]
impl Ort for Http {
    async fn ort(
        &mut self,
        Spec {
            latency,
            response_size,
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
            let latency_ms = latency.as_millis() as i64;

            tracing::trace!(latency_ms, response_size);
            uri.path_and_query(
                http::uri::PathAndQuery::try_from(
                    format!("/?latency_ms={}&size={}", latency_ms, response_size).as_str(),
                )
                .expect("query must be valid"),
            )
        };

        let rsp = self
            .client
            .request(
                http::Request::builder()
                    .uri(uri.build().unwrap())
                    .body(hyper::Body::default())
                    .unwrap(),
            )
            .await?;

        let data = hyper::body::to_bytes(rsp.into_body()).await?;

        Ok(Reply { data })
    }
}
