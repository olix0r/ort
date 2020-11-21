use crate::report::Report;
use hdrhistogram::Histogram;
use parking_lot::RwLock;
use serde_json as json;
use std::{convert::Infallible, net::SocketAddr, sync::Arc};

#[derive(Clone)]
pub struct Admin {
    histogram: Arc<RwLock<Histogram<u64>>>,
}

impl Admin {
    pub fn new(histogram: Arc<RwLock<Histogram<u64>>>) -> Self {
        Self { histogram }
    }

    pub async fn serve(&self, addr: SocketAddr) -> Result<(), hyper::Error> {
        let admin = self.clone();
        hyper::Server::bind(&addr)
            .serve(hyper::service::make_service_fn(move |_| {
                let admin = admin.clone();
                async move {
                    Ok::<_, Infallible>(hyper::service::service_fn(
                        move |req: hyper::Request<hyper::Body>| {
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
        req: hyper::Request<hyper::Body>,
    ) -> Result<hyper::Response<hyper::Body>, Infallible> {
        match req.uri().path() {
            "/live" | "/ready" => {
                if let hyper::Method::GET | hyper::Method::HEAD = *req.method() {
                    return Ok(hyper::Response::builder()
                        .status(hyper::StatusCode::NO_CONTENT)
                        .body(hyper::Body::default())
                        .unwrap());
                }
            }

            "/report.json" => {
                if let hyper::Method::GET = *req.method() {
                    let report = Report::from(&*self.histogram.read());
                    return Ok(hyper::Response::builder()
                        .status(hyper::StatusCode::OK)
                        .header(hyper::header::CONTENT_TYPE, "application/json")
                        .body(json::to_vec_pretty(&report).unwrap().into())
                        .unwrap());
                }
            }

            _ => {}
        }

        Ok(hyper::Response::builder()
            .status(hyper::StatusCode::NOT_FOUND)
            .body(hyper::Body::default())
            .unwrap())
    }
}
