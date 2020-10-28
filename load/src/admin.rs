use std::{convert::Infallible, net::SocketAddr};

#[derive(Clone, Default)]
pub struct Admin(());

impl Admin {
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
                    .status(http::StatusCode::OK)
                    .body(hyper::Body::default())
                    .unwrap());
            }
        }

        Ok(http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(hyper::Body::default())
            .unwrap())
    }
}
