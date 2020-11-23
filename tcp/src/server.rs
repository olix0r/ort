use crate::{muxer, preface, ReplyCodec, SpecCodec};
use futures::{future, prelude::*, stream::FuturesUnordered};
use ort_core::{Error, Ort};
use std::{future::Future, net::SocketAddr};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::error;

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

    pub async fn serve<C>(self, addr: SocketAddr, mut closed: C) -> Result<(), Error>
    where
        C: Future<Output = ()> + Unpin,
    {
        let mut serving = FuturesUnordered::new();
        let mut lis = tokio::net::TcpListener::bind(addr).await?;

        loop {
            tokio::select! {
                _ = serving.next() => {}

                _ = (&mut closed) => break,

                acc = lis.accept() => {
                    let (rio, wio) = match acc {
                        Ok((sock, _addr)) => sock.into_split(),
                        Err(error) => {
                            error!(%error, "Failed to accept connectoin");
                            continue;
                        }
                    };

                    let decode = preface::Codec::from(muxer::FramedDecode::from(SpecCodec::default()));
                    let encode = muxer::FramedEncode::from(ReplyCodec::default());
                    let (mut rx, muxer) = muxer::spawn_server(
                        FramedRead::new(rio, decode),
                        FramedWrite::new(wio, encode),
                        future::pending(),
                        self.buffer_capacity,
                    );

                    let srv = self.inner.clone();
                    let buffer_capacity = self.buffer_capacity;

                    let server = tokio::spawn(async move {
                        let mut in_flight = FuturesUnordered::new();
                        loop {
                            if in_flight.len() == buffer_capacity {
                                in_flight.next().await;
                            }

                            tokio::select! {
                                _ = in_flight.next() => {}

                                next = rx.next() => match next {
                                    None => break,
                                    Some((spec, tx)) => {
                                        let mut srv = srv.clone();
                                        let h = tokio::spawn(async move {
                                            let reply = srv.ort(spec).await?;
                                            let _ = tx.send(reply);
                                            Ok::<(), Error>(())
                                        });
                                        in_flight.push(h.map(|res| match res {
                                            Ok(Ok(())) => {},
                                            Ok(Err(error)) => error!(%error, "Service failed"),
                                            Err(error) => error!(%error, "Task failed"),
                                        }));
                                    }
                                }
                            }
                        }

                        while let Some(()) = in_flight.next().await {}
                    });

                    serving.push(async move {
                        let (m, r) = tokio::join!(muxer, server);
                        let () = r?;
                        m
                    })
                }
            }
        }

        while let Some(_) = serving.next().await {}

        Ok(())
    }
}
