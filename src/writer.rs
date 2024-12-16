use std::{
    collections::HashMap,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, SimplexStream, WriteHalf};
use uuid::Uuid;

use super::MultiReader;

pub struct MultiWriter<S: AsyncWrite + Unpin> {
    pub(super) uuid: Uuid,
    inner: S,
    pub(super) writers: HashMap<Uuid, WriteHalf<SimplexStream>>,
    buffer_size: usize,
}

// all that is written into me (and by consequence, inner) will be written into the subscribers as well
impl<S: AsyncWrite + Unpin> MultiWriter<S> {
    pub fn new(stream: S, uuid: Option<Uuid>, buffer_size: Option<usize>) -> Self {
        Self {
            uuid: uuid.unwrap_or(Uuid::new_v4()),
            inner: stream,
            writers: HashMap::new(),
            buffer_size: buffer_size.unwrap_or(64),
        }
    }

    pub fn make_reader(
        &mut self,
        uuid: Option<Uuid>,
    ) -> io::Result<(ReadHalf<SimplexStream>, Uuid)> {
        let (reader, writer) = tokio::io::simplex(self.buffer_size);
        let uuid = uuid.unwrap_or(Uuid::new_v4());

        self.add_writer(writer, uuid)?;

        Ok((reader, uuid))
    }

    pub fn add_writer(&mut self, writer: WriteHalf<SimplexStream>, uuid: Uuid) -> io::Result<()> {
        match self.writers.insert(uuid, writer) {
            Some(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Writer already attached.",
            )),
            None => Ok(()),
        }
    }

    pub fn reader(
        &mut self,
        buffer_size: Option<usize>,
    ) -> io::Result<MultiReader<ReadHalf<SimplexStream>>> {
        let (reader, uuid) = self.make_reader(None)?;

        Ok(MultiReader::new(reader, Some(uuid), buffer_size))
    }

    pub fn attach<R: AsyncRead + Unpin>(&mut self, reader: &mut MultiReader<R>) -> io::Result<()> {
        let (sender, _) = reader.make_writer(Some(self.uuid))?;

        self.add_writer(sender, reader.uuid)
    }

    pub fn detach<R: AsyncRead + Unpin>(&mut self, reader: &MultiReader<R>) -> io::Result<()> {
        let uuid = reader.uuid;

        if !self.writers.contains_key(&uuid) && reader.readers.contains_key(&self.uuid) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "You are trying to detach the reader that created you, that is not allowed. Please make a separate stream of data and then attach the reader to this writer.",
            ));
        }

        self.writers
            .remove(&uuid)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Writer not attached."))
            .map(|_| ())
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for MultiWriter<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // writers + inner
        let mut results = Vec::with_capacity(self.writers.len() + 1);

        // inner write
        results.push(Pin::new(&mut self.inner).poll_write(cx, buf));

        // extend writers
        results.extend(
            self.writers
                .iter_mut()
                .map(|(_, subscriber)| Pin::new(subscriber).poll_write(cx, buf))
                .collect::<Vec<_>>(),
        );

        let mut min_written = usize::MAX;
        let mut any_pending = false;

        for result in results {
            match result {
                Poll::Ready(Ok(n)) => {
                    min_written = min_written.min(n);
                }
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => {
                    any_pending = true;
                    break;
                }
            }
        }

        if any_pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(min_written))
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Collect flush results
        let mut results = Vec::with_capacity(1 + self.writers.len());

        // Poll main stream flush
        results.push(Pin::new(&mut self.inner).poll_flush(cx));

        // Poll all other streams flush
        results.extend(
            self.writers
                .iter_mut()
                .map(|(_, subscriber)| Pin::new(subscriber).poll_flush(cx))
                .collect::<Vec<_>>(),
        );

        // Check results
        for result in results {
            match result {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Collect flush results
        let mut results = Vec::with_capacity(1 + self.writers.len());

        // Poll main stream flush
        results.push(Pin::new(&mut self.inner).poll_shutdown(cx));

        // Poll all other streams flush
        results.extend(
            self.writers
                .iter_mut()
                .map(|(_, subscriber)| Pin::new(subscriber).poll_shutdown(cx))
                .collect::<Vec<_>>(),
        );

        // Check results
        for result in results {
            match result {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }
}
