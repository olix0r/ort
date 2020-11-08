// XXX this doesn't really handle connection errors properly...

use crate::{ReplyCodec, SpecCodec, PREFIX};
use futures::prelude::*;
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use std::sync::Arc;
use tokio::{
    net::{tcp, TcpStream},
    prelude::*,
    sync::Mutex,
};
use tokio_util::codec::{FramedRead, FramedWrite};

#[derive(Clone)]
pub struct MakeTcp {}

#[derive(Clone)]
pub struct Tcp(Arc<Mutex<Inner>>);

struct Inner {
    write: FramedWrite<tcp::OwnedWriteHalf, SpecCodec>,
    read: FramedRead<tcp::OwnedReadHalf, ReplyCodec>,
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
        let mut stream = TcpStream::connect(target).await?;
        stream.set_nodelay(true)?;
        stream.write(PREFIX.as_bytes()).await?;
        let (read, write) = stream.into_split();
        Ok(Tcp(Arc::new(Mutex::new(Inner {
            write: FramedWrite::new(write, SpecCodec::default()),
            read: FramedRead::new(read, ReplyCodec::default()),
        }))))
    }
}

#[async_trait::async_trait]
impl Ort for Tcp {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let Inner {
            ref mut read,
            ref mut write,
        } = *self.0.lock().await;
        write.send(spec).await?;
        read.try_next()
            .await?
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected).into())
    }
}
