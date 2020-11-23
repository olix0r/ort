use bytes::{Buf, BufMut, BytesMut};
use futures::{prelude::*, stream::FuturesUnordered};
use std::{collections::HashMap, future::Future};
use tokio::{
    io,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};

#[derive(Default, Debug)]
pub struct Muxer<E, D> {
    max_in_flight: usize,
    encoder: FramedEncode<E>,
    decoder: FramedDecode<D>,
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Frame<T> {
    pub id: u64,
    pub value: T,
}

#[derive(Default, Debug)]
pub struct FramedEncode<E> {
    inner: E,
}

#[derive(Debug)]
pub struct FramedDecode<D> {
    inner: D,
    state: DecodeState,
}

#[derive(Debug)]
enum DecodeState {
    Init,
    Head { id: u64 },
}

#[allow(warnings)]
pub fn spawn_client<Req, Rsp, W, E, R, D>(
    mut write: FramedWrite<W, E>,
    mut read: FramedRead<R, D>,
    max_in_flight: usize,
) -> mpsc::Sender<(Req, oneshot::Sender<Rsp>)>
where
    Req: Send + 'static,
    Rsp: Send + 'static,
    E: Encoder<Frame<Req>, Error = io::Error> + Send + 'static,
    D: Decoder<Item = Frame<Rsp>, Error = io::Error> + Send + 'static,
    R: io::AsyncRead + Send + Unpin + 'static,
    W: io::AsyncWrite + Send + Unpin + 'static,
{
    let (tx, mut rx) = mpsc::channel(max_in_flight);

    tokio::spawn(async move {
        let mut next_id = 1u64;
        let mut in_flight = HashMap::<u64, oneshot::Sender<Rsp>>::new();

        loop {
            if next_id == std::u64::MAX {
                break;
            }

            tokio::select! {
                r = rx.next() => match r {
                    None => break,
                    Some((value, rsp_tx)) => {
                        let id = next_id;
                        next_id += 1;
                        if let Err(e) = write.send(Frame { id, value }).await {
                            return Err(e);
                        }
                        in_flight.insert(id, rsp_tx);
                    }
                },

                rsp = read.try_next() => match rsp? {
                    None => break,
                    Some(Frame { id, value }) => {
                        match in_flight.remove(&id) {
                            None => unimplemented!(),
                            Some(tx) => {
                                let _ = tx.send(value);
                            }
                        }
                    }
                },
            }
        }

        Ok(())
    });

    tx
}

pub fn spawn_server<Req, Rsp, D, R, E, W, C>(
    mut read: FramedRead<R, D>,
    mut write: FramedWrite<W, E>,
    mut closed: C,
    max_in_flight: usize,
) -> (
    mpsc::Receiver<(Req, oneshot::Sender<Rsp>)>,
    tokio::task::JoinHandle<io::Result<()>>,
)
where
    Req: Send + 'static,
    Rsp: Send + 'static,
    E: Encoder<Frame<Rsp>, Error = io::Error> + Send + 'static,
    D: Decoder<Item = Frame<Req>, Error = io::Error> + Send + 'static,
    R: io::AsyncRead + Send + Unpin + 'static,
    W: io::AsyncWrite + Send + Unpin + 'static,
    C: Future<Output = ()> + Send + Unpin + 'static,
{
    let (tx, rx) = mpsc::channel(max_in_flight);

    let handle = tokio::spawn(async move {
        let mut pending = FuturesUnordered::new();
        loop {
            while pending.len() == max_in_flight {
                if let Some(rsp) = pending.try_next().await? {
                    write.send(rsp).await?;
                }
            }

            tokio::select! {
                _ = (&mut closed) => break,

                res = pending.try_next() => {
                    if let Some(rsp) = res? {
                        write.send(rsp).await?;
                    }
                }

                r = read.try_next() => {
                    let Frame { id, value } = match r? {
                        Some(f) => f,
                        None => break,
                    };

                    let mut tx = tx.clone();
                    let p = tokio::spawn(async move {
                        let (rsp_tx, rsp_rx) = oneshot::channel();
                        match tx.send((value, rsp_tx)).await {
                            Err(_) => {
                                Err(io::Error::new(io::ErrorKind::ConnectionAborted, "Server disconnected"))
                            }
                            Ok(()) => {
                                rsp_rx.await
                                    .map(move |value| Frame { id, value })
                                    .map_err(|_| {
                                        io::Error::new(io::ErrorKind::ConnectionAborted, "Server dropped response")
                                    })
                            }
                        }
                    }).map(|res| match res {
                        Ok(res) => res,
                        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
                    });

                    pending.push(p.map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            e.to_string(),
                        )
                    }));
                }
            }
        }

        while let Some(rsp) = pending.try_next().await? {
            write.send(rsp).await?;
        }

        Ok(())
    });

    (rx, handle)
}

// === impl FramedDecode ===

impl<D> From<D> for FramedDecode<D> {
    fn from(inner: D) -> Self {
        Self {
            inner,
            state: DecodeState::Init,
        }
    }
}

impl<D: Default> Default for FramedDecode<D> {
    fn default() -> Self {
        Self::from(D::default())
    }
}

impl<D: Decoder> Decoder for FramedDecode<D> {
    type Item = Frame<D::Item>;
    type Error = D::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame<D::Item>>, D::Error> {
        let id = match self.state {
            DecodeState::Init => {
                if src.len() < 8 {
                    return Ok(None);
                }
                src.get_u64()
            }
            DecodeState::Head { id } => {
                self.state = DecodeState::Init;
                id
            }
        };

        match self.inner.decode(src)? {
            Some(value) => {
                return Ok(Some(Frame { id, value }));
            }
            None => {
                self.state = DecodeState::Head { id };
                return Ok(None);
            }
        }
    }
}

// === impl FramedEncode ===

impl<E> From<E> for FramedEncode<E> {
    fn from(inner: E) -> Self {
        Self { inner }
    }
}

impl<T, C: Encoder<T>> Encoder<Frame<T>> for FramedEncode<C> {
    type Error = C::Error;

    fn encode(
        &mut self,
        Frame { id, value }: Frame<T>,
        dst: &mut BytesMut,
    ) -> Result<(), C::Error> {
        dst.reserve(8);
        dst.put_u64(id);
        self.inner.encode(value, dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio_util::codec::LengthDelimitedCodec;

    #[tokio::test]
    async fn roundtrip() {
        let mux0 = Frame {
            id: 1,
            value: Bytes::from_static(b"abcde"),
        };
        let mux1 = Frame {
            id: 1,
            value: Bytes::from_static(b"fghij"),
        };

        let mut buf = BytesMut::with_capacity(100);

        let mut enc = FramedEncode::<LengthDelimitedCodec>::default();
        enc.encode(mux0.clone(), &mut buf).expect("must encode");
        enc.encode(mux1.clone(), &mut buf).expect("must encode");

        let mut dec = FramedDecode::<LengthDelimitedCodec>::default();
        let d0 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        let d1 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        assert_eq!(d0.id, mux0.id);
        assert_eq!(d0.value.freeze(), mux0.value);
        assert_eq!(d1.id, mux1.id);
        assert_eq!(d1.value.freeze(), mux1.value);
    }
}
