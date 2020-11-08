use ort_core::{Error, Ort, Reply, Spec};
use std::{convert::Infallible, net::SocketAddr};
use tokio_02::time;

#[derive(Clone, Debug)]
pub struct Server<O> {
    inner: O,
}

impl<O: Ort> Server<O> {
    pub fn new(inner: O) -> Self {
        Self { inner }
    }

    async fn handle(
        mut self,
        req: http::Request<hyper::Body>,
    ) -> Result<http::Response<hyper::Body>, Error> {
        if req.method() == http::Method::GET {
            let mut spec = Spec::default();

            if let Some(q) = req.uri().query() {
                for kv in q.split('&') {
                    let mut kv = kv.splitn(2, '=');
                    match kv.next() {
                        Some("latency_ms") => {
                            if let Some(ms) = kv.next().and_then(|v| v.parse::<u64>().ok()) {
                                spec.latency = time::Duration::from_millis(ms);
                            }
                        }
                        Some("size") => {
                            if let Some(sz) = kv.next().and_then(|v| v.parse::<usize>().ok()) {
                                spec.response_size = sz;
                            }
                        }
                        Some(_) | None => {}
                    }
                }
            }

            let Reply { data } = self.inner.ort(spec).await?;
            return http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/octet-stream")
                .body(data.into())
                .map_err(Into::into);
        }

        http::Response::builder()
            .status(http::StatusCode::BAD_REQUEST)
            .body(hyper::Body::default())
            .map_err(Into::into)
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), Error> {
        let srv = self.clone();
        hyper::Server::bind(&addr)
            .serve(hyper::service::make_service_fn(move |_| {
                let srv = srv.clone();
                async move {
                    Ok::<_, Infallible>(hyper::service::service_fn(
                        move |req: http::Request<hyper::Body>| srv.clone().handle(req),
                    ))
                }
            }))
            .await?;

        Ok(())
    }
}
