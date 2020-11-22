use crate::{Muxish, MuxishCodec, ReplyCodec, SpecCodec, PREFIX, PREFIX_LEN};
use futures::prelude::*;
use ort_core::{Error, Ort};
use std::net::SocketAddr;
use tokio::prelude::*;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::info;

pub struct Server<O>(O);

impl<O: Ort> Server<O> {
    pub fn new(inner: O) -> Self {
        Self(inner)
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), Error> {
        let mut lis = tokio::net::TcpListener::bind(addr).await?;

        loop {
            let mut srv = self.0.clone();
            let (mut stream, _addr) = lis.accept().await?;
            tokio::spawn(async move {
                let (mut sock_rx, sock_tx) = stream.split();

                let mut prefix = [0u8; PREFIX_LEN];
                debug_assert_eq!(prefix.len(), PREFIX.len());
                let _sz = match sock_rx.read_exact(&mut prefix).await {
                    Ok(sz) => sz,
                    Err(error) => {
                        info!(%error, "Could not read prefix");
                        return;
                    }
                };

                if prefix != PREFIX {
                    info!(?prefix, expected = ?PREFIX, "Client isn't speaking our language");
                    return;
                }

                let mut read = FramedRead::new(sock_rx, MuxishCodec::from(SpecCodec::default()));
                let mut write = FramedWrite::new(sock_tx, MuxishCodec::from(ReplyCodec::default()));

                loop {
                    match read.next().await {
                        None => return,
                        Some(Err(error)) => {
                            info!(%error, "Failed reading message");
                            return;
                        }
                        Some(Ok(Muxish { id, value: spec })) => {
                            let reply = match srv.ort(spec).await {
                                Ok(reply) => reply,
                                Err(error) => {
                                    info!(%error, "Failed replying");
                                    return;
                                }
                            };
                            if let Err(error) = write.send(Muxish { id, value: reply }).await {
                                info!(%error, "Failed writing reply");
                                return;
                            }
                        }
                    }
                }
            });
        }
    }
}
