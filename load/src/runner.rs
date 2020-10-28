use crate::{proto, RateLimit};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::debug_span;
use tracing_futures::Instrument;

#[async_trait::async_trait]
pub trait MakeClient {
    type Client: Client;

    async fn make_client(&mut self) -> Self::Client;
}

#[async_trait::async_trait]
pub trait Client {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status>;
}

#[derive(Copy, Clone)]
pub struct Runner {
    clients: usize,
    streams: usize,
    rate_limit: RateLimit,
}

impl Runner {
    pub fn new(clients: usize, streams: usize, rate_limit: RateLimit) -> Self {
        Self {
            clients,
            streams,
            rate_limit,
        }
    }

    pub async fn run<C, G>(self, connect: C)
    where
        C: MakeClient<Client = G> + Clone + Send + 'static,
        G: Client + Clone + Send + 'static,
    {
        let Self {
            clients,
            streams,
            rate_limit,
        } = self;

        if clients == 0 || streams == 0 {
            return;
        }

        let limit = rate_limit.spawn();

        for client in 0..clients {
            let limit = limit.clone();
            let mut connect = connect.clone();
            tokio::spawn(
                async move {
                    let client = connect.make_client().await;
                    let streams = Arc::new(Semaphore::new(streams));
                    let limit = limit.clone();
                    loop {
                        let permits =
                            (streams.clone().acquire_owned().await, limit.acquire().await);
                        let mut client = client.clone();
                        tokio::spawn(
                            async move {
                                // TODO generate request params (latency, size, error).
                                let spec = proto::ResponseSpec {
                                    result: Some(proto::response_spec::Result::Success(
                                        proto::response_spec::Success::default(),
                                    )),
                                    ..Default::default()
                                };

                                let _ = client.get(spec).await;

                                drop(permits);
                            }
                            .in_current_span(),
                        );
                    }
                }
                .instrument(debug_span!("client", id = %client)),
            );
        }
    }
}
