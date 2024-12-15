use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::{
    io::{self, AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};
use uuid::Uuid;

use super::{PipeWriter, SenderWriter};

#[pin_project]
pub struct PipeReader<S: AsyncRead + Unpin> {
    #[pin]
    inner: S,

    pub uuid: Uuid,

    receivers: HashMap<Uuid, mpsc::Receiver<Vec<u8>>>,

    buffer: Vec<u8>,
    position: usize,

    buffer_size: usize,
}

// all that is read from the inner writer will be sent to all subscribers
impl<S: AsyncRead + Unpin> PipeReader<S> {
    pub fn new(stream: S, uuid: Option<Uuid>, buffer_size: Option<usize>) -> Self {
        Self {
            uuid: uuid.unwrap_or(Uuid::new_v4()),
            inner: stream,
            receivers: HashMap::new(),
            buffer: Vec::new(),
            position: 0,
            buffer_size: buffer_size.unwrap_or(64),
        }
    }

    pub fn make_subscriber(&mut self) -> (mpsc::Sender<Vec<u8>>, Uuid) {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        let uuid = Uuid::new_v4();
        self.receivers.insert(uuid, rx);

        (tx, uuid)
    }

    pub fn add_receiver(&mut self, rx: mpsc::Receiver<Vec<u8>>, uuid: Uuid) {
        self.receivers.insert(uuid, rx);
    }

    // make a writer that writes to this reader
    pub fn writer(&mut self, buffer_size: Option<usize>) -> PipeWriter<SenderWriter> {
        let (sender, uuid) = self.make_subscriber();
        PipeWriter::new(SenderWriter::new(sender), Some(uuid), buffer_size)
    }

    pub fn attach<W: AsyncWrite + Unpin>(&mut self, writer: &mut PipeWriter<W>) {
        let (receiver, uuid) = writer.make_receiver();
        self.add_receiver(receiver, uuid);
    }

    pub fn detach<W: AsyncWrite + Unpin>(&mut self, writer: &PipeWriter<W>) {
        let uuid = writer.uuid;
        self.receivers.remove(&uuid);
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PipeReader<S> {
    // todo i can prob make this function better
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.project();

        // if we have data in the buffer, serve it first
        if *this.position < this.buffer.len() {
            let available = &this.buffer[*this.position..];
            let to_copy = available.len().min(buf.remaining());
            buf.put_slice(&available[..to_copy]);
            *this.position += to_copy;
            return Poll::Ready(Ok(()));
        }

        // create a future that polls all receivers
        let recv_futures = this
            .receivers
            .iter_mut()
            .map(|(_, rx)| rx.poll_recv(cx))
            .collect::<Vec<_>>();

        // also poll the inner reader
        let mut inner_buf = vec![0u8; 4096];
        let mut read_buf = ReadBuf::new(&mut inner_buf);
        let inner_poll = this.inner.poll_read(cx, &mut read_buf);

        // check inner read first, it always has priority
        match inner_poll {
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled();
                if !filled.is_empty() {
                    *this.buffer = filled.to_vec();
                    *this.position = 0;
                    let to_copy = this.buffer.len().min(buf.remaining());
                    buf.put_slice(&this.buffer[..*this.position + to_copy]);
                    *this.position += to_copy;
                    return Poll::Ready(Ok(()));
                }
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }

        // check all receivers
        for poll_result in recv_futures {
            match poll_result {
                Poll::Ready(Some(data)) => {
                    *this.buffer = data;
                    *this.position = 0;
                    let to_copy = this.buffer.len().min(buf.remaining());
                    buf.put_slice(&this.buffer[..*this.position + to_copy]);
                    *this.position += to_copy;
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(None) => {
                    // channel closed, could remove it from the list
                    continue;
                }
                Poll::Pending => continue,
            }
        }

        // if we got here, everything was pending so we are pending as well...
        Poll::Pending
    }
}
