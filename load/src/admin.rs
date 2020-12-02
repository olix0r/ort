use crate::metrics::Report;
use linkerd2_metrics::Serve;
use std::{io, net::SocketAddr};

#[derive(Clone)]
pub struct Admin(Serve<Report>);

impl Admin {
    pub fn new(report: Report) -> Self {
        Self(Serve::new(report))
    }

    pub async fn serve(&self, addr: SocketAddr) -> Result<(), hyper::Error> {
        let admin = self.clone();
        hyper::Server::bind(&addr)
            .serve(hyper::service::make_service_fn(move |_| {
                let admin = admin.clone();
                async move {
                    Ok::<_, io::Error>(hyper::service::service_fn(
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
    ) -> io::Result<hyper::Response<hyper::Body>> {
        match req.uri().path() {
            "/live" | "/ready" => {
                if let hyper::Method::GET | hyper::Method::HEAD = *req.method() {
                    return Ok(hyper::Response::builder()
                        .status(hyper::StatusCode::NO_CONTENT)
                        .body(hyper::Body::default())
                        .unwrap());
                }
            }

            "/metrics" => {
                if let hyper::Method::GET = *req.method() {
                    return self.0.serve(req);
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
