#![deny(warnings, rust_2018_idioms)]

use std::{convert::Infallible, net::SocketAddr, sync::Arc, time::Duration};
use structopt::StructOpt;
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::Semaphore,
    time::delay_for,
};
use tracing::{debug_span, info_span, warn};
use tracing_futures::Instrument;

mod proto {
    tonic::include_proto!("ortiofay.olix0r.net");
}

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {
    #[structopt(long)]
    request_limit: Option<usize>,

    #[structopt(short, long, parse(try_from_str), default_value = "0.0.0.0:8000")]
    admin_addr: SocketAddr,

    #[structopt(subcommand)]
    flavor: Flavor,
}

#[derive(Clone, Debug, StructOpt)]
#[structopt()]
enum Flavor {
    // // Generate HTTP/1.1 load
    // Http {},
    // Generate gRPC load
    Grpc {
        #[structopt(short, long, default_value = "1")]
        connections: usize,

        #[structopt(short, long, default_value = "1")]
        streams: usize,

        target: http::Uri,
    },
}

impl Load {
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        match self.flavor {
            Flavor::Grpc {
                connections,
                streams,
                target,
            } => {
                if connections == 0 || streams == 0 {
                    return Ok(());
                }
                for worker in 0..connections {
                    let target = target.clone();
                    tokio::spawn(
                        async move {
                            let client =
                                Self::connect(target.clone(), Duration::from_secs(1)).await;
                            let streams = Arc::new(Semaphore::new(streams));
                            loop {
                                let mut client = client.clone();
                                let permit = streams.clone().acquire_owned().await;
                                tokio::spawn(
                                    async move {
                                        let req = tonic::Request::new(proto::ResponseSpec {
                                            result: Some(proto::response_spec::Result::Success(
                                                proto::response_spec::Success::default(),
                                            )),
                                            ..Default::default()
                                        });
                                        let _ = client.get(req).await;
                                        drop(permit);
                                    }
                                    .in_current_span(),
                                );
                            }
                        }
                        .instrument(info_span!("worker", id = %worker)),
                    );
                }

                let admin_addr = self.admin_addr;
                tokio::spawn(
                    async move {
                        hyper::Server::bind(&admin_addr)
                            .serve(hyper::service::make_service_fn(move |_| async {
                                Ok::<_, Infallible>(hyper::service::service_fn(Admin::handle))
                            }))
                            .await
                            .expect("Admin server must not fail")
                    }
                    .instrument(debug_span!("admin")),
                );

                signal(SignalKind::terminate())?.recv().await;

                Ok(())
            }
        }
    }

    async fn connect(
        target: http::Uri,
        backoff: Duration,
    ) -> proto::ortiofay_client::OrtiofayClient<tonic::transport::Channel> {
        loop {
            match proto::ortiofay_client::OrtiofayClient::connect(target.clone()).await {
                Ok(client) => return client,
                Err(error) => {
                    warn!(%error, "Failed to connect");
                    delay_for(backoff).await;
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
struct Admin;

impl Admin {
    async fn handle(
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
