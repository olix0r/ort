use crate::next_or_pending;
use bytes::{Buf, BufMut, BytesMut};
use futures::{prelude::*, stream::FuturesUnordered};
use linkerd2_drain::Watch as Drain;
use std::collections::HashMap;
use tokio::{
    io,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{Decoder, Encoder};
use tracing::{debug, debug_span, error, info, trace};
use tracing_futures::Instrument;

#[derive(Default, Debug)]
pub struct Muxer<E, D> {
    buffer_capacity: usize,
    encoder: FramedEncode<E>,
    decoder: FramedDecode<D>,
}

#[derive(Debug)]
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

pub fn spawn_client<Req, Rsp, W, R>(
    mut write: W,
    mut read: R,
    buffer_capacity: usize,
) -> mpsc::Sender<(Req, oneshot::Sender<Rsp>)>
where
    Req: Send + 'static,
    Rsp: Send + 'static,
    W: Sink<Frame<Req>, Error = io::Error> + Send + Unpin + 'static,
    R: Stream<Item = io::Result<Frame<Rsp>>> + Send + Unpin + 'static,
{
    let (req_tx, mut req_rx) = mpsc::channel(buffer_capacity);

    tokio::spawn(
        async move {
            let mut next_id = 1u64;
            let mut in_flight = HashMap::<u64, oneshot::Sender<Rsp>>::new();

            loop {
                if next_id == std::u64::MAX {
                    info!("Client exhausted request IDs");
                    break;
                }

                tokio::select! {
                    // Read requests from the stream and write them on the socket.
                    // Stash the response oneshot for when the response is read.
                    req = req_rx.next() => match req {
                        Some((value, rsp_tx)) => {
                            let id = next_id;
                            next_id += 1;
                            trace!(id, "Dispatching request");
                            let f = in_flight.entry(id);
                            if let std::collections::hash_map::Entry::Occupied(_) = f {
                                error!(id, "Request ID already in-flight");
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "Request ID is already in-flight",
                                ));
                            }
                            if let Err(error) = write.send(Frame { id, value }).await {
                                error!(id, %error, "Failed to write response");
                                return Err(error);
                            }
                            f.or_insert(rsp_tx);
                        }
                        None => {
                            debug!("Client dropped its send handle");
                            break;
                        }
                    },

                    // Read responses from the socket and send them back to the
                    // client.
                    rsp = read.try_next() => match rsp? {
                        Some(Frame { id, value }) => {
                            trace!(id, "Dispatching response");
                            match in_flight.remove(&id) {
                                Some(tx) => {
                                    let _ = tx.send(value);
                                }
                                None => return Err(io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "Response for unknown request",
                                )),
                            }
                        }
                        None => {
                            debug!(in_flight=in_flight.len(), "Server closed");
                            if in_flight.len() == 0 {
                                return Ok(());
                            } else {
                                return Err(io::Error::new(
                                    io::ErrorKind::ConnectionReset,
                                    "Server closed",
                                ));
                            }
                        }
                    },
                }
            }

            debug!("Allowing pending responses to complete");

            // We shan't be sending any more requests. Keep reading
            // responses, though.
            drop((req_rx, write));

            // Satisfy remaining responses.
            while let Some(Frame { id, value }) = read.try_next().await? {
                match in_flight.remove(&id) {
                    Some(tx) => {
                        let _ = tx.send(value);
                    }
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Response for unknown request",
                        ));
                    }
                }
            }
            if !in_flight.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "Some requests did not receive a response",
                ));
            }

            Ok(())
        }
        .in_current_span(),
    );

    req_tx
}

pub fn spawn_server<Req, Rsp, R, W>(
    mut read: R,
    mut write: W,
    drain: Drain,
    buffer_capacity: usize,
) -> (
    mpsc::Receiver<(Req, oneshot::Sender<Rsp>)>,
    tokio::task::JoinHandle<io::Result<()>>,
)
where
    Req: Send + 'static,
    Rsp: Send + 'static,
    R: Stream<Item = io::Result<Frame<Req>>> + Send + Unpin + 'static,
    W: Sink<Frame<Rsp>, Error = io::Error> + Send + Unpin + 'static,
{
    let (mut tx, rx) = mpsc::channel(buffer_capacity);

    let drain = drain.clone();

    let handle = tokio::spawn(async move {
        tokio::pin! {
            let closed = drain.signal();
        }

        let mut last_id = 0u64;
        let mut in_flight = FuturesUnordered::new();
        loop {
            tokio::select! {
                shutdown = (&mut closed) => {
                    debug!("Shutdown signaled; draining in-flight requests");
                    drop(read);
                    drop(tx);
                    while let Some(Frame { id, value }) = in_flight.try_next().await? {
                        trace!(id, "In-flight response completed");
                        write.send(Frame { id, value }).await?;
                    }
                    debug!("In-flight requests completed");
                    drop(shutdown);
                    return Ok(());
                }

                req = next_or_pending(&mut in_flight) => {
                    let Frame { id, value } = req?;
                    trace!(id, "In-flight response completed");
                    if let Err(error) = write.send(Frame { id, value }).await {
                        error!(%error, "Write failed");
                        return Err(error);
                    }
                }

                msg = read.try_next() => {
                    let Frame { id, value } = match msg? {
                        Some(f) => f,
                        None => {
                            trace!("Draining in-flight responses after client stream completed.");
                            while let Some(Frame { id, value }) = in_flight.try_next().await? {
                                trace!(id, "In-flight response completed");
                                write.send(Frame { id, value }).await?;
                            }
                            return Ok(());
                        }
                    };

                    if id <= last_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Request ID too low",
                        ));
                    }
                    last_id = id;

                    trace!(id, "Dispatching request");
                    let (rsp_tx, rsp_rx) = oneshot::channel();
                    if tx.send((value, rsp_tx)).await.is_err() {
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "Lost service",
                        ));
                    }
                    in_flight.push(rsp_rx.map(move |v| match v {
                        Ok(value) => Ok(Frame { id, value }),
                        Err(_) => Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "Server dropped response",
                        )),
                    }));
                }
            }
        }
    }.instrument(debug_span!("mux")));

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
