#![deny(warnings, rust_2018_idioms)]

pub mod client;
pub mod server;

use bytes::{Buf, BufMut, BytesMut};
use ort_core::{Error, Reply, Spec};
use tokio::{io, time};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

const PREFIX_LEN: usize = 23;
static PREFIX: &[u8] = b"ort.olix0r.net/load\r\n\r\n";

#[derive(Copy, Clone, Debug, Default, PartialEq)]
struct Muxish<T> {
    id: u64,

    value: T,
}

struct MuxishCodec<C> {
    codec: C,
    state: MuxishState,
}

enum MuxishState {
    Init,
    Head { id: u64 },
}

#[derive(Default)]
struct SpecCodec(());

struct ReplyCodec(LengthDelimitedCodec);

// === impl MuxishCodec ===

impl<C> From<C> for MuxishCodec<C> {
    fn from(codec: C) -> Self {
        Self {
            codec,
            state: MuxishState::Init,
        }
    }
}

impl<C: Default> Default for MuxishCodec<C> {
    fn default() -> Self {
        Self {
            codec: C::default(),
            state: MuxishState::Init,
        }
    }
}

impl<C: Decoder> Decoder for MuxishCodec<C> {
    type Item = Muxish<C::Item>;
    type Error = C::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Muxish<C::Item>>, C::Error> {
        let id = match self.state {
            MuxishState::Init => {
                if src.len() < 8 {
                    return Ok(None);
                }
                src.get_u64()
            }
            MuxishState::Head { id } => {
                self.state = MuxishState::Init;
                id
            }
        };

        match self.codec.decode(src)? {
            Some(value) => {
                return Ok(Some(Muxish { id, value }));
            }
            None => {
                self.state = MuxishState::Head { id };
                return Ok(None);
            }
        }
    }
}

impl<T, C: Encoder<T>> Encoder<Muxish<T>> for MuxishCodec<C> {
    type Error = C::Error;

    fn encode(
        &mut self,
        Muxish { id, value }: Muxish<T>,
        dst: &mut BytesMut,
    ) -> Result<(), C::Error> {
        dst.reserve(8);
        dst.put_u64(id);
        self.codec.encode(value, dst)
    }
}

// === impl SpecCodec ===

impl Decoder for SpecCodec {
    type Item = Spec;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Spec>> {
        if src.len() < 4 + 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Message too short",
            ));
        }
        let ms = src.get_u32();
        let sz = src.get_u32();
        Ok(Some(Spec {
            latency: time::Duration::from_millis(ms as u64),
            response_size: sz as usize,
        }))
    }
}

impl Encoder<Spec> for SpecCodec {
    type Error = Error;

    fn encode(&mut self, spec: Spec, dst: &mut BytesMut) -> Result<(), Error> {
        dst.reserve(4 + 4);
        dst.put_u32(spec.latency.as_millis() as u32);
        dst.put_u32(spec.response_size as u32);
        Ok(())
    }
}

// === impl ReplyCodec ===

impl Default for ReplyCodec {
    fn default() -> Self {
        let frames = LengthDelimitedCodec::builder()
            .max_frame_length(std::u32::MAX as usize)
            .length_field_length(4)
            .new_codec();
        Self(frames)
    }
}

impl Decoder for ReplyCodec {
    type Item = Reply;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Reply>, Error> {
        match self.0.decode(src)? {
            None => Ok(None),
            Some(buf) => Ok(Some(Reply { data: buf.freeze() })),
        }
    }
}

impl Encoder<Reply> for ReplyCodec {
    type Error = Error;

    fn encode(&mut self, Reply { data }: Reply, dst: &mut BytesMut) -> Result<(), Error> {
        self.0.encode(data, dst)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn roundtrip_mux() {
        let mux0 = Muxish {
            id: 1,
            value: Bytes::from_static(b"abcde"),
        };
        let mux1 = Muxish {
            id: 1,
            value: Bytes::from_static(b"fghij"),
        };

        let mut buf = BytesMut::with_capacity(100);

        let mut enc = MuxishCodec::<LengthDelimitedCodec>::default();
        enc.encode(mux0.clone(), &mut buf).expect("must encode");
        enc.encode(mux1.clone(), &mut buf).expect("must encode");

        let mut dec = MuxishCodec::<LengthDelimitedCodec>::default();
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

    #[tokio::test]
    async fn roundtrip_spec() {
        let spec0 = Spec {
            latency: time::Duration::from_millis(1),
            response_size: 3,
        };
        let spec1 = Spec {
            latency: time::Duration::from_millis(2),
            response_size: 4,
        };

        let mut buf = BytesMut::with_capacity(100);

        let mut enc = SpecCodec::default();
        enc.encode(spec0, &mut buf).expect("must encode");
        enc.encode(spec1, &mut buf).expect("must encode");

        let mut dec = SpecCodec::default();
        assert_eq!(
            dec.decode(&mut buf)
                .expect("must decode")
                .expect("must decode"),
            spec0
        );
        assert_eq!(
            dec.decode(&mut buf)
                .expect("must decode")
                .expect("must decode"),
            spec1
        );
    }

    #[tokio::test]
    async fn roundtrip_reply() {
        let reply0 = Reply {
            data: Bytes::from_static(b"abcdef"),
        };
        let reply1 = Reply {
            data: Bytes::from_static(b"ghijkl"),
        };

        let mut buf = BytesMut::with_capacity(100);

        let mut enc = ReplyCodec::default();
        enc.encode(reply0.clone(), &mut buf).expect("must encode");
        enc.encode(reply1.clone(), &mut buf).expect("must encode");

        let mut dec = ReplyCodec::default();
        assert_eq!(
            dec.decode(&mut buf)
                .expect("must decode")
                .expect("must decode"),
            reply0
        );
        assert_eq!(
            dec.decode(&mut buf)
                .expect("must decode")
                .expect("must decode"),
            reply1
        );
    }
}
