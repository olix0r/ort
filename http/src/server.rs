use futures::prelude::*;
use linkerd2_drain::Watch as Drain;
use ort_core::{Error, Ort, Reply, Spec};
use std::{convert::Infallible, net::SocketAddr};
use tokio::time;

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

    pub async fn serve(self, addr: SocketAddr, drain: Drain) -> Result<(), Error> {
        let svc = hyper::service::make_service_fn(move |_| {
            let handler = self.clone();
            async move {
                Ok::<_, Infallible>(hyper::service::service_fn(
                    move |req: http::Request<hyper::Body>| handler.clone().handle(req),
                ))
            }
        });

        let (close, closed) = tokio::sync::oneshot::channel();
        tokio::pin! {
            let srv = hyper::Server::bind(&addr)
                .serve(svc)
                .with_graceful_shutdown(closed.map(|_| ()));
        }

        tokio::select! {
            _ = (&mut srv) => {}
            handle = drain.signal() => {
                let _ = close.send(());
                handle.release_after(srv).await?;
            }
        }

        Ok(())
    }
}
