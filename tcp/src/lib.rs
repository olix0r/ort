#![deny(warnings, rust_2018_idioms)]

pub mod client;
pub mod muxer;
pub mod preface;
pub mod server;

use bytes::{Buf, BufMut, BytesMut};
use ort_core::{Reply, Spec};
use tokio::{io, time};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

#[derive(Default)]
struct SpecCodec(());

struct ReplyCodec(LengthDelimitedCodec);

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
    type Error = io::Error;

    fn encode(&mut self, spec: Spec, dst: &mut BytesMut) -> io::Result<()> {
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
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Reply>, io::Error> {
        match self.0.decode(src)? {
            None => Ok(None),
            Some(buf) => Ok(Some(Reply { data: buf.freeze() })),
        }
    }
}

impl Encoder<Reply> for ReplyCodec {
    type Error = io::Error;

    fn encode(&mut self, Reply { data }: Reply, dst: &mut BytesMut) -> io::Result<()> {
        self.0.encode(data, dst)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

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
