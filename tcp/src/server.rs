use crate::{muxer, ReplyCodec, SpecCodec, PREFIX, PREFIX_LEN};
use futures::{prelude::*, stream::FuturesUnordered};
use ort_core::{Error, Ort};
use std::net::SocketAddr;
use tokio::prelude::*;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, info};

pub struct Server<O>(O);

impl<O: Ort> Server<O> {
    pub fn new(inner: O) -> Self {
        Self(inner)
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), Error> {
        let mut lis = tokio::net::TcpListener::bind(addr).await?;

        loop {
            let srv = self.0.clone();
            let (mut stream, _addr) = lis.accept().await?;
            tokio::spawn(async move {
                let (mut sock_rx, sock_tx) = stream.split();

                let mut prefix = [0u8; PREFIX_LEN];
                debug_assert_eq!(prefix.len(), PREFIX.len());
                let _sz = sock_rx.read_exact(&mut prefix).await?;

                if prefix != PREFIX {
                    info!(?prefix, expected = ?PREFIX, "Client isn't speaking our language");
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid protocol preface",
                    ));
                }

                let mut read = FramedRead::new(sock_rx, muxer::Codec::from(SpecCodec::default()));
                let mut write =
                    FramedWrite::new(sock_tx, muxer::Codec::from(ReplyCodec::default()));

                let mut pending = FuturesUnordered::new();

                loop {
                    tokio::select! {
                        msg = read.try_next() => {
                            if let Some(muxer::Frame { id, value: spec }) = msg? {
                                let mut srv = srv.clone();
                                let fut = tokio::spawn(async move {
                                    srv.ort(spec)
                                        .await
                                        .map(move |value| muxer::Frame { id, value })
                                        .map_err(|e| {
                                            io::Error::new(
                                                io::ErrorKind::InvalidData,
                                                e.to_string(),
                                            )
                                        })
                                }).map(|res| match res {
                                    Ok(res) => res,
                                    Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
                                });

                                pending.push(fut.map_err(|_| {
                                    io::Error::new(
                                        io::ErrorKind::ConnectionReset,
                                        "Connection closed",
                                    )
                                }));
                            }
                        },

                        res = pending.next() => match res {
                            None => {},
                            Some(Ok(reply)) => {
                                write.send(reply).await?;
                            }
                            Some(Err(error)) => {
                                error!(?error, "Runtime failure");
                                break;
                            }
                        }
                    }
                }

                Ok(())
            });
        }
    }
}
