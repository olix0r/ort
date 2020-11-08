use crate::{ReplyCodec, SpecCodec, PREFIX};
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

                let mut prefix = Vec::<u8>::with_capacity(PREFIX.as_bytes().len());
                let sz = match sock_rx.read_exact(prefix.as_mut()).await {
                    Ok(sz) => sz,
                    Err(error) => {
                        info!(%error, "Could not read prefix");
                        return;
                    }
                };

                if sz != PREFIX.as_bytes().len() || prefix.as_slice() != PREFIX.as_bytes() {
                    info!("Client isn't speaking our language");
                    return;
                }

                let mut read = FramedRead::new(sock_rx, SpecCodec::default());
                let mut write = FramedWrite::new(sock_tx, ReplyCodec::default());

                loop {
                    match read.next().await {
                        None => return,
                        Some(Err(error)) => {
                            info!(%error, "Failed reading message");
                            return;
                        }
                        Some(Ok(spec)) => {
                            let reply = match srv.ort(spec).await {
                                Ok(reply) => reply,
                                Err(error) => {
                                    info!(%error, "Failed replying");
                                    return;
                                }
                            };
                            if let Err(error) = write.send(reply).await {
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
