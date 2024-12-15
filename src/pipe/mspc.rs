use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};

#[pin_project]
pub struct SenderWriter {
    sender: mpsc::Sender<Vec<u8>>,
    buffer: Vec<u8>,
}

impl SenderWriter {
    pub fn new(sender: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            sender,
            buffer: Vec::new(),
        }
    }
}

impl AsyncWrite for SenderWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        match this.sender.try_send(buf.to_vec()) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(_)) => Poll::Pending,
            Err(_) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "receiver disconnected",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // mpsc channels don't need flushing
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[pin_project]
pub struct ReceiverReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    position: usize,
}

impl ReceiverReader {
    pub fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buffer: Vec::new(),
            position: 0,
        }
    }
}

impl AsyncRead for ReceiverReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.project();

        if *this.position < this.buffer.len() {
            let available = &this.buffer[*this.position..];
            let to_copy = available.len().min(buf.remaining());
            buf.put_slice(&available[..to_copy]);
            *this.position += to_copy;
            return Poll::Ready(Ok(()));
        }

        match this.receiver.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                *this.buffer = data;
                *this.position = 0;
                let to_copy = this.buffer.len().min(buf.remaining());
                buf.put_slice(&this.buffer[..*this.position + to_copy]);
                *this.position += to_copy;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}
