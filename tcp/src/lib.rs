#![deny(warnings, rust_2018_idioms)]

pub mod client;
pub mod server;

use bytes::BytesMut;
use ort_core::{Error, Reply, Spec};
use serde_json as json;
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

const PREFIX_LEN: usize = 23;
static PREFIX: &str = "ort.olix0r.net/load\r\n\r\n";

struct SpecCodec(LengthDelimitedCodec);

struct ReplyCodec(LengthDelimitedCodec);

// === impl SpecCodec ===

impl Default for SpecCodec {
    fn default() -> Self {
        let frames = LengthDelimitedCodec::builder()
            .max_frame_length(std::u32::MAX as usize)
            .length_field_length(4)
            .new_codec();
        Self(frames)
    }
}

impl Decoder for SpecCodec {
    type Item = Spec;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Spec>, Error> {
        match self.0.decode(src)? {
            None => Ok(None),
            Some(buf) => {
                let spec = json::from_slice(buf.freeze().as_ref())?;
                Ok(Some(spec))
            }
        }
    }
}

impl Encoder<Spec> for SpecCodec {
    type Error = Error;

    fn encode(&mut self, spec: Spec, dst: &mut BytesMut) -> Result<(), Error> {
        let buf = json::to_vec(&spec)?;
        self.0.encode(buf.into(), dst)?;
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
