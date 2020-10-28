use crate::report::Report;
use hdrhistogram::Histogram;
use serde_json as json;
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

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
        match req.uri().path() {
            "/live" | "/ready" => {
                if let http::Method::GET | http::Method::HEAD = *req.method() {
                    return Ok(http::Response::builder()
                        .status(http::StatusCode::NO_CONTENT)
                        .body(hyper::Body::default())
                        .unwrap());
                }
            }

            "/report.json" => {
                if let http::Method::GET = *req.method() {
                    let report = {
                        let h = self.histogram.read().await;
                        Report::from(&*h)
                    };
                    return Ok(http::Response::builder()
                        .status(http::StatusCode::OK)
                        .header(http::header::CONTENT_TYPE, "application/json")
                        .body(json::to_vec_pretty(&report).unwrap().into())
                        .unwrap());
                }
            }

            _ => {}
        }

        Ok(http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(hyper::Body::default())
            .unwrap())
    }
}
