use crate::{muxer, preface, ReplyCodec, SpecCodec};
use futures::prelude::*;
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use std::sync::Arc;
use tokio::{
    net::{tcp, TcpStream},
    sync::Mutex,
};
use tokio_util::codec::{FramedRead, FramedWrite};

#[derive(Clone)]
pub struct MakeTcp {}

#[derive(Clone)]
pub struct Tcp(Arc<Mutex<Inner>>);

struct Inner {
    next_id: u64,
    write: FramedWrite<tcp::OwnedWriteHalf, preface::Codec<muxer::FramedEncode<SpecCodec>>>,
    read: FramedRead<tcp::OwnedReadHalf, muxer::FramedDecode<ReplyCodec>>,
}

impl MakeTcp {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl MakeOrt<String> for MakeTcp {
    type Ort = Tcp;

    async fn make_ort(&mut self, target: String) -> Result<Tcp, Error> {
        let stream = TcpStream::connect(target).await?;
        stream.set_nodelay(true)?;
        let (read, write) = stream.into_split();
        Ok(Tcp(Arc::new(Mutex::new(Inner {
            next_id: 1,
            write: FramedWrite::new(write, Default::default()),
            read: FramedRead::new(read, Default::default()),
        }))))
    }
}

#[async_trait::async_trait]
impl Ort for Tcp {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let Inner {
            ref mut next_id,
            ref mut read,
            ref mut write,
        } = *self.0.lock().await;

        if *next_id == std::u64::MAX {
            return Err(Exhausted(()).into());
        }
        let id = *next_id;
        *next_id += 1;

        write.send(muxer::Frame { id, value: spec }).await?;

        read.try_next()
            .await?
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected).into())
            .map(|muxer::Frame { value, .. }| value)
    }
}

#[derive(Debug)]
struct Exhausted(());

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ran out of ids")
    }
}

impl std::error::Error for Exhausted {}
