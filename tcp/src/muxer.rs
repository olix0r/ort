use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Frame<T> {
    pub id: u64,
    pub value: T,
}

pub struct Codec<C> {
    codec: C,
    state: State,
}

enum State {
    Init,
    Head { id: u64 },
}

// === impl Codec ===

impl<C> From<C> for Codec<C> {
    fn from(codec: C) -> Self {
        Self {
            codec,
            state: State::Init,
        }
    }
}

impl<C: Default> Default for Codec<C> {
    fn default() -> Self {
        Self {
            codec: C::default(),
            state: State::Init,
        }
    }
}

impl<C: Decoder> Decoder for Codec<C> {
    type Item = Frame<C::Item>;
    type Error = C::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame<C::Item>>, C::Error> {
        let id = match self.state {
            State::Init => {
                if src.len() < 8 {
                    return Ok(None);
                }
                src.get_u64()
            }
            State::Head { id } => {
                self.state = State::Init;
                id
            }
        };

        match self.codec.decode(src)? {
            Some(value) => {
                return Ok(Some(Frame { id, value }));
            }
            None => {
                self.state = State::Head { id };
                return Ok(None);
            }
        }
    }
}

impl<T, C: Encoder<T>> Encoder<Frame<T>> for Codec<C> {
    type Error = C::Error;

    fn encode(
        &mut self,
        Frame { id, value }: Frame<T>,
        dst: &mut BytesMut,
    ) -> Result<(), C::Error> {
        dst.reserve(8);
        dst.put_u64(id);
        self.codec.encode(value, dst)
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

        let mut enc = Codec::<LengthDelimitedCodec>::default();
        enc.encode(mux0.clone(), &mut buf).expect("must encode");
        enc.encode(mux1.clone(), &mut buf).expect("must encode");

        let mut dec = Codec::<LengthDelimitedCodec>::default();
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
