use rand::{rngs::SmallRng, RngCore};
use std::{convert::Infallible, net::SocketAddr};
use tokio_02::time;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

async fn handle<B: hyper::body::HttpBody>(
    req: http::Request<B>,
    mut rng: SmallRng,
) -> Result<http::Response<hyper::Body>, Error> {
    if req.method() == http::Method::GET {
        let mut latency = time::Duration::new(0, 0);
        let mut rsp_size = 0;
        let mut status = http::StatusCode::OK;

        if let Some(q) = req.uri().query() {
            for kv in q.split('&') {
                let mut kv = kv.splitn(2, '=');
                match kv.next() {
                    Some("latency_ms") => {
                        if let Some(ms) = kv.next().and_then(|v| v.parse::<u64>().ok()) {
                            latency = time::Duration::from_millis(ms);
                        }
                    }
                    Some("size") => {
                        if let Some(sz) = kv.next().and_then(|v| v.parse::<usize>().ok()) {
                            rsp_size = sz;
                        }
                    }
                    Some("error") => {
                        if let Some(code) =
                            kv.next().and_then(|v| v.parse::<http::StatusCode>().ok())
                        {
                            status = code;
                        }
                    }
                    Some(_) | None => {}
                }
            }
        }

        // Start sleeping before generating the body so that time isn't added to
        // our latency.
        tracing::trace!(?latency, "Sleeping");
        let sleep = time::delay_for(latency);

        let mut data = Vec::<u8>::with_capacity(rsp_size);
        rng.fill_bytes(data.as_mut());

        sleep.await;

        return http::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .body(data.into())
            .map_err(Into::into);
    }

    http::Response::builder()
        .status(http::StatusCode::BAD_REQUEST)
        .body(hyper::Body::default())
        .map_err(Into::into)
}

pub async fn serve(addr: SocketAddr, rng: SmallRng) -> Result<(), Error> {
    hyper::Server::bind(&addr)
        .serve(hyper::service::make_service_fn(move |_| {
            let rng = rng.clone();
            async move {
                Ok::<_, Infallible>(hyper::service::service_fn(
                    move |req: http::Request<hyper::Body>| handle(req, rng.clone()),
                ))
            }
        }))
        .await?;
    Ok(())
}
