use std::{
    collections::HashMap,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use uuid::Uuid;

use super::{PipeReader, ReceiverReader};

pub struct PipeWriter<S: AsyncWrite + Unpin> {
    pub uuid: Uuid,
    inner: S,
    subscribers: HashMap<Uuid, mpsc::Sender<Vec<u8>>>,
    buffer_size: usize,
}

// all that is written into me (and by consequence, inner) will be written into the subscribers as well
impl<S: AsyncWrite + Unpin> PipeWriter<S> {
    pub fn new(stream: S, uuid: Option<Uuid>, buffer_size: Option<usize>) -> Self {
        Self {
            uuid: uuid.unwrap_or(Uuid::new_v4()),
            inner: stream,
            subscribers: HashMap::new(),
            buffer_size: buffer_size.unwrap_or(64),
        }
    }

    pub fn make_receiver(&mut self) -> (mpsc::Receiver<Vec<u8>>, Uuid) {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        let uuid = Uuid::new_v4();
        self.subscribers.insert(uuid, tx);

        (rx, uuid)
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Vec<u8>>, uuid: Uuid) {
        self.subscribers.insert(uuid, tx);
    }

    pub fn reader(&mut self, buffer_size: Option<usize>) -> PipeReader<ReceiverReader> {
        let (receiver, uuid) = self.make_receiver();
        PipeReader::new(ReceiverReader::new(receiver), Some(uuid), buffer_size)
    }

    pub fn attach<R: AsyncRead + Unpin>(&mut self, reader: &mut PipeReader<R>) {
        let (sender, uuid) = reader.make_subscriber();
        self.add_subscriber(sender, uuid);
    }

    pub fn detach<R: AsyncRead + Unpin>(&mut self, reader: &PipeReader<R>) {
        let uuid = reader.uuid;
        self.subscribers.remove(&uuid);
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PipeWriter<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // over here we can afford to be sequential, waiting on the inner's read to write
        match Pin::new(&mut self.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                let _ = self
                    .subscribers
                    .iter_mut()
                    .map(|(_, subscriber)| subscriber.try_send(buf[..n].to_vec()).is_ok())
                    .collect::<Vec<_>>();

                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
