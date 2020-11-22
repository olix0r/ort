use bytes::{Buf, BufMut, BytesMut};
use futures::{prelude::*, stream::FuturesUnordered};
use std::collections::HashMap;
use tokio::{
    io,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};
use tracing::error;

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

// === impl Muxer ===

impl<E, D> Muxer<E, D> {
    pub fn new(enc: E, dec: D, max_in_flight: usize) -> Self {
        Self {
            max_in_flight,
            encoder: enc.into(),
            decoder: dec.into(),
        }
    }

    #[allow(warnings)]
    pub fn spawn_client<Req, R, W>(
        self,
        rio: R,
        wio: W,
    ) -> mpsc::Sender<(Req, oneshot::Receiver<Result<D::Item, D::Error>>)>
    where
        Req: Send + 'static,
        E: Encoder<Req> + Send + 'static,
        D: Decoder + Send + 'static,
        D::Item: Send + 'static,
        D::Error: Send + 'static,
        R: io::AsyncRead + Send + 'static,
        W: io::AsyncWrite + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel(self.max_in_flight);

        tokio::spawn(async move {
            let mut next_id = 1u64;
            let mut write = FramedWrite::new(wio, self.encoder);
            let mut read = FramedRead::new(rio, self.decoder);
            let mut in_flight = HashMap::<u64, oneshot::Sender<Result<D::Item, D::Error>>>::new();
            loop {
                let _ = rx.next().await;
            }
        });

        tx
    }

    pub fn spawn_server<Rsp, R, W>(
        self,
        rio: R,
        wio: W,
    ) -> mpsc::Receiver<(D::Item, oneshot::Sender<Rsp>)>
    where
        Rsp: Send + 'static,
        E: Encoder<Rsp, Error = io::Error> + Send + 'static,
        D: Decoder<Error = io::Error> + Send + 'static,
        D::Item: Send + 'static,
        R: io::AsyncRead + Send + Unpin + 'static,
        W: io::AsyncWrite + Send + Unpin + 'static,
    {
        let (tx, rx) = mpsc::channel(self.max_in_flight);
        let mut read = FramedRead::new(rio, self.decoder);
        let mut write = FramedWrite::new(wio, self.encoder);

        tokio::spawn(async move {
            let mut pending = FuturesUnordered::new();

            loop {
                tokio::select! {
                    r = read.try_next() => {
                        let msg = match r {
                            Ok(res) => res,
                            Err(e) => return Err(e),
                        };
                        if let Some(Frame { id, value }) = msg {
                            let mut tx = tx.clone();
                            let fut = tokio::spawn(async move {
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

                            pending.push(fut.map_err(|e| {
                                io::Error::new(
                                    io::ErrorKind::ConnectionAborted,
                                    e.to_string(),
                                )
                            }));
                        }
                    },

                    res = pending.next() => match res {
                        None => {},
                        Some(Ok(rsp)) => {
                            write.send(rsp).await?;
                        }
                        Some(Err(error)) => {
                            error!(?error, "Runtime failure");
                            return Ok(());
                        }
                    }
                }
            }
        });

        rx
    }
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
        let d1 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        let d2 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        assert_eq!(d1.id, mux0.id);
        assert_eq!(d1.value.freeze(), mux0.value);
        assert_eq!(d2.value.freeze(), mux1.value);
    }
}
