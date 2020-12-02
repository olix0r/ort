//! TODO TCP clients shoudl automatically reconnect, but they don't

use crate::{muxer, preface, ReplyCodec, SpecCodec};
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use tokio::{
    io,
    net::TcpStream,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{debug, debug_span};

#[derive(Clone)]
pub struct MakeTcp {
    buffer_capacity: usize,
}

#[derive(Clone)]
pub struct Tcp {
    tx: mpsc::Sender<(Spec, oneshot::Sender<Reply>)>,
}

impl MakeTcp {
    pub fn new(buffer_capacity: usize) -> Self {
        Self { buffer_capacity }
    }
}

#[async_trait::async_trait]
impl MakeOrt<String> for MakeTcp {
    type Ort = Tcp;

    async fn make_ort(&mut self, target: String) -> Result<Tcp, Error> {
        debug!(%target, "Initializing a new connection");
        let stream = TcpStream::connect(target).await?;
        stream.set_nodelay(true)?;

        let local = stream.local_addr()?;
        let peer = stream.peer_addr()?;
        let (rio, wio) = stream.into_split();
        let write = FramedWrite::new(
            wio,
            preface::Codec::from(muxer::FramedEncode::from(SpecCodec::default())),
        );
        let read = FramedRead::new(rio, muxer::FramedDecode::from(ReplyCodec::default()));
        let tx = debug_span!("conn", %local, %peer)
            .in_scope(|| muxer::spawn_client(write, read, self.buffer_capacity));

        Ok(Tcp { tx })
    }
}

#[async_trait::async_trait]
impl Ort for Tcp {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send((spec, tx))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "Muxer lost"))?;
        rx.await.map_err(|_| {
            io::Error::new(io::ErrorKind::NotConnected, "Muxer dropped response").into()
        })
    }
}
