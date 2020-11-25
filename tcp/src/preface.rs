use bytes::{Buf, BufMut, BytesMut};
use tokio::io;
use tokio_util::codec::{Decoder, Encoder};

static PREFACE: &[u8] = b"ort.olix0r.net/load\r\n\r\n";

#[derive(Debug)]
pub struct Codec<C> {
    preface: &'static [u8],
    inner: C,
    state: State,
}

#[derive(Debug)]
enum State {
    Init,
    Prefaced,
}

// === impl Codec ===

impl<C> From<C> for Codec<C> {
    fn from(inner: C) -> Self {
        Self {
            inner,
            preface: PREFACE,
            state: State::Init,
        }
    }
}

impl<C: Default> Default for Codec<C> {
    fn default() -> Self {
        Self::from(C::default())
    }
}

impl<D: Decoder> Decoder for Codec<D> {
    type Item = D::Item;
    type Error = D::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<D::Item>, D::Error> {
        loop {
            match self.state {
                State::Prefaced => {
                    return self.inner.decode(src);
                }
                State::Init => {
                    if src.len() < self.preface.len() {
                        return Ok(None);
                    }
                    if &src[0..self.preface.len()] != self.preface {
                        return Err(D::Error::from(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid protocol header",
                        )));
                    }
                    src.advance(self.preface.len());
                    self.state = State::Prefaced;
                }
            }
        }
    }
}

impl<T, E: Encoder<T>> Encoder<T> for Codec<E> {
    type Error = E::Error;

    fn encode(&mut self, value: T, dst: &mut BytesMut) -> Result<(), E::Error> {
        loop {
            match self.state {
                State::Prefaced => {
                    return self.inner.encode(value, dst);
                }
                State::Init => {
                    dst.reserve(self.preface.len());
                    dst.put(self.preface);
                    self.state = State::Prefaced;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio_util::codec::LengthDelimitedCodec;

    #[tokio::test]
    async fn roundtrip() {
        let b0 = Bytes::from_static(b"abcde");
        let b1 = Bytes::from_static(b"fghij");

        let mut buf = BytesMut::with_capacity(100);

        let mut enc = Codec::from(LengthDelimitedCodec::default());
        enc.encode(b0.clone(), &mut buf).expect("must encode");
        enc.encode(b1.clone(), &mut buf).expect("must encode");

        let mut dec = Codec::from(LengthDelimitedCodec::default());
        let d0 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        let d1 = dec
            .decode(&mut buf)
            .expect("must decode")
            .expect("must decode");
        assert_eq!(d0.freeze(), b0);
        assert_eq!(d1.freeze(), b1);
    }
}
