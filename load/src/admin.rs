use crate::report::Report;
use hdrhistogram::sync::SyncHistogram;
use serde_json as json;
use std::{convert::Infallible, net::SocketAddr, sync::Arc};

#[derive(Clone)]
pub struct Admin {
    histogram: Arc<SyncHistogram<u64>>,
}

impl Admin {
    pub fn new(h: impl Into<Arc<SyncHistogram<u64>>>) -> Self {
        Self {
            histogram: h.into(),
        }
    }

    pub async fn serve(&self, addr: SocketAddr) -> Result<(), hyper::Error> {
        let admin = self.clone();
        hyper::Server::bind(&addr)
            .serve(hyper::service::make_service_fn(move |_| {
                let admin = admin.clone();
                async move {
                    Ok::<_, Infallible>(hyper::service::service_fn(
                        move |req: http::Request<hyper::Body>| {
                            let admin = admin.clone();
                            async move { admin.handle(req).await }
                        },
                    ))
                }
            }))
            .await
    }

    async fn handle(
        &self,
        req: http::Request<hyper::Body>,
    ) -> Result<http::Response<hyper::Body>, Infallible> {
        if let "/live" | "/ready" = req.uri().path() {
            if let http::Method::GET | http::Method::HEAD = *req.method() {
                return Ok(http::Response::builder()
                    .status(http::StatusCode::NO_CONTENT)
                    .body(hyper::Body::default())
                    .unwrap());
            }
        }

        if let "/report.json" = req.uri().path() {
            if let http::Method::GET = *req.method() {
                let report = Report::from(self.histogram.as_ref());
                let json = json::to_vec_pretty(&report).unwrap();
                return Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .header(http::header::CONTENT_TYPE, "application/json")
                    //.header(http::header::CONTENT_LENGTH, json.len().to_string())
                    .body(json.into())
                    .unwrap());
            }
        }

        Ok(http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(hyper::Body::default())
            .unwrap())
    }
}
