use crate::{muxer, next_or_pending, preface, ReplyCodec, SpecCodec};
use futures::{prelude::*, stream::FuturesUnordered};
use linkerd_drain::Watch as Drain;
use ort_core::{Error, Ort};
use std::net::SocketAddr;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{debug, debug_span, error, trace};
use tracing_futures::Instrument;

pub struct Server<O> {
    inner: O,
    buffer_capacity: usize,
}

impl<O: Ort> Server<O> {
    pub fn new(inner: O) -> Self {
        Self {
            inner,
            buffer_capacity: 100_000,
        }
    }

    pub async fn serve(self, addr: SocketAddr, drain: Drain) -> Result<(), Error> {
        let mut serving = FuturesUnordered::new();
        let lis = tokio::net::TcpListener::bind(addr).await?;

        tokio::pin! {
            let closed = drain.clone().signaled();
        }

        loop {
            tokio::select! {
                shutdown = (&mut closed) => {
                    debug!("Letting all connections complete before shutdown");
                    while let Some(_) = serving.next().await {}
                    drop(shutdown);
                    return Ok(());
                }

                _ = next_or_pending(&mut serving) => {}

                acc = lis.accept() => {
                    let ((rio, wio), peer) = match acc {
                        Ok((sock, peer)) => {
                            debug!(%peer, "Client connected");
                            (sock.into_split(), peer)
                        }
                        Err(error) => {
                            error!(%error, "Failed to accept connection");
                            continue;
                        }
                    };

                    let span = debug_span!("conn", %peer);

                    let decode = preface::Codec::from(muxer::FramedDecode::from(SpecCodec::default()));
                    let encode = muxer::FramedEncode::from(ReplyCodec::default());
                    let (mut rx, muxer) = span.in_scope(|| muxer::spawn_server(
                        FramedRead::new(rio, decode),
                        FramedWrite::new(wio, encode),
                        drain.clone(),
                        self.buffer_capacity,
                    ));

                    let srv = self.inner.clone();
                    let drain = drain.clone();

                    let server = tokio::spawn(async move {
                        tokio::pin! {
                            let closed = drain.signaled();
                        }

                        let mut in_flight = FuturesUnordered::new();
                        loop {
                            tokio::select! {
                                shutdown = (&mut closed) => {
                                    debug!("Draining inflight requests before shutdown");
                                    drop(rx);
                                    while let Some(()) = in_flight.next().await {};
                                    drop(shutdown);
                                    return;
                                }

                                _ = next_or_pending(&mut in_flight) => {
                                    trace!("Response completed");
                                }

                                next = rx.recv() => match next {
                                    None => {
                                        debug!("Client closed; draining in-flight requests");
                                        while let Some(()) = in_flight.next().await {};
                                        return;
                                    }
                                    Some((spec, tx)) => {
                                        let mut srv = srv.clone();
                                        let h = tokio::spawn(async move {
                                            let reply = srv.ort(spec).await?;
                                            let _ = tx.send(reply);
                                            Ok::<(), Error>(())
                                        }.instrument(debug_span!("req")));
                                        in_flight.push(h.map(|res| match res {
                                            Ok(Ok(())) => {},
                                            Ok(Err(error)) => error!(%error, "Service failed"),
                                            Err(error) => error!(%error, "Task failed"),
                                        }));
                                    }
                                }
                            }
                        }
                    }.instrument(span));

                    serving.push(async move {
                        let (m, r) = tokio::join!(muxer, server);
                        debug!(?m, ?r, "Connection complete");
                        let () = r?;
                        m
                    })
                }
            }
        }
    }
}
