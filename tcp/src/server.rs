use crate::{muxer::Muxer, ReplyCodec, SpecCodec, PREFIX, PREFIX_LEN};
use futures::{prelude::*, stream::FuturesUnordered};
use ort_core::{Error, Ort};
use std::net::SocketAddr;
use tokio::prelude::*;
use tracing::info;

pub struct Server<O> {
    inner: O,
    max_in_flight: usize,
}

impl<O: Ort> Server<O> {
    pub fn new(inner: O) -> Self {
        Self {
            inner,
            max_in_flight: 100_000,
        }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), Error> {
        let mut lis = tokio::net::TcpListener::bind(addr).await?;
        loop {
            let (sock, _addr) = lis.accept().await?;
            let srv = self.inner.clone();
            let max_in_flight = self.max_in_flight;
            tokio::spawn(async move {
                let (mut sock_rx, sock_tx) = sock.into_split();

                let mut prefix = [0u8; PREFIX_LEN];
                debug_assert_eq!(prefix.len(), PREFIX.len());
                let _sz = sock_rx.read_exact(&mut prefix).await?;

                if prefix != PREFIX {
                    info!(?prefix, expected = ?PREFIX, "Client isn't speaking our language");
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid protocol preface",
                    )
                    .into());
                }

                let muxer = Muxer::new(ReplyCodec::default(), SpecCodec::default(), max_in_flight);
                let mut rx = muxer.spawn_server(sock_rx, sock_tx);

                let mut in_flight = FuturesUnordered::new();

                loop {
                    if in_flight.len() == max_in_flight {
                        in_flight.try_next().await?;
                    }

                    tokio::select! {
                         res = in_flight.try_next() => {
                             if let Err(e) = res {
                                 return Err(e);
                             }
                         }
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
                                     Ok(Ok(())) => Ok(()),
                                     Ok(Err(e)) => Err(e),
                                     Err(e) => Err(e.into()),
                                 }));
                             }
                         }
                    }
                }

                while let Some(()) = in_flight.try_next().await? {}

                Ok(())
            });
        }
    }
}
