use crate::{muxer, preface, ReplyCodec, SpecCodec};
use futures::{prelude::*, stream::FuturesUnordered};
use ort_core::{Error, Ort};
use std::net::SocketAddr;
use tokio_util::codec::{FramedRead, FramedWrite};

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
            let decode = preface::Codec::from(muxer::FramedDecode::from(SpecCodec::default()));
            let encode = muxer::FramedEncode::from(ReplyCodec::default());
            let (rio, wio) = sock.into_split();
            let mut rx = muxer::spawn_server(
                FramedRead::new(rio, decode),
                FramedWrite::new(wio, encode),
                self.max_in_flight,
            );

            let srv = self.inner.clone();
            let max_in_flight = self.max_in_flight;
            tokio::spawn(async move {
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
