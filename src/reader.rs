use std::{
    collections::HashMap,
    pin::{Pin, pin},
    task::{Context, Poll},
    vec,
};

use pin_project::pin_project;
use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf, ReadHalf, SimplexStream, WriteHalf};
use tokio_util::bytes::BytesMut;
use uuid::Uuid;

use super::MultiWriter;

#[pin_project]
pub struct MultiReader<S: AsyncRead + Unpin> {
    #[pin]
    inner: S,

    pub(super) uuid: Uuid,

    pub(super) readers: HashMap<Uuid, ReadHalf<SimplexStream>>,

    buffer_size: usize,

    buffer: BytesMut,
    position: usize,
}

// all that is read from the inner writer will be sent to all subscribers
impl<S: AsyncRead + Unpin> MultiReader<S> {
    pub fn new(stream: S, uuid: Option<Uuid>, buffer_size: Option<usize>) -> Self {
        let buffer_size = buffer_size.unwrap_or(64);
        Self {
            uuid: uuid.unwrap_or(Uuid::new_v4()),
            inner: stream,
            readers: HashMap::new(),
            buffer_size,

            buffer: BytesMut::with_capacity(buffer_size),
            position: 0,
        }
    }

    pub fn make_writer(
        &mut self,
        uuid: Option<Uuid>,
    ) -> io::Result<(WriteHalf<SimplexStream>, Uuid)> {
        let (reader, writer) = tokio::io::simplex(self.buffer_size);
        let uuid = uuid.unwrap_or(Uuid::new_v4());
        self.add_reader(reader, uuid)?;

        Ok((writer, uuid))
    }

    pub(super) fn add_reader(
        &mut self,
        reader: ReadHalf<SimplexStream>,
        uuid: Uuid,
    ) -> io::Result<()> {
        match self.readers.insert(uuid, reader) {
            Some(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Reader already attached.",
            )),
            None => Ok(()),
        }
    }

    // make a writer that writes to this reader
    pub fn writer(
        &mut self,
        buffer_size: Option<usize>,
    ) -> io::Result<MultiWriter<WriteHalf<SimplexStream>>> {
        let (writer, uuid) = self.make_writer(None)?;

        Ok(MultiWriter::new(writer, Some(uuid), buffer_size))
    }

    pub fn attach<W: AsyncWrite + Unpin>(&mut self, writer: &mut MultiWriter<W>) -> io::Result<()> {
        let (reader, _) = writer.make_reader(Some(self.uuid))?;

        self.add_reader(reader, writer.uuid)
    }

    pub fn detach<W: AsyncWrite + Unpin>(&mut self, writer: &MultiWriter<W>) -> io::Result<()> {
        let uuid = writer.uuid;

        if !self.readers.contains_key(&uuid) && writer.writers.contains_key(&self.uuid) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "You are trying to detach the writer that created you, that is not allowed. Please make a separate stream of data and then attach the writer to this reader.",
            ));
        }

        self.readers
            .remove(&uuid)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Writer not attached."))
            .map(|_| ())
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for MultiReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();

        // if we have data in the buffer, serve it first
        if *this.position < this.buffer.len() {
            let available = &this.buffer[*this.position..];

            // we'll either copy all that's on my buffer or all that we can fit into the buffer
            let to_copy = std::cmp::min(available.len(), buf.remaining());

            // place the data we can copy into the buffer, advancing the buffer's position accordingly
            buf.put_slice(&available[..to_copy]);

            // our own position has advanced by the amount we copied
            *this.position += to_copy;

            return Poll::Ready(Ok(()));
        }

        // inner
        let mut inner_buf = vec![0u8; 4096];
        let mut inner_read_buf = ReadBuf::new(&mut inner_buf);
        let inner_poll = Pin::new(&mut this.inner).poll_read(cx, &mut &mut inner_read_buf);
        match inner_poll {
            Poll::Ready(Ok(())) => {
                let filled = inner_read_buf.filled();

                if !filled.is_empty() {
                    *this.buffer = BytesMut::from(filled);
                    *this.position = 0;
                    let to_copy = std::cmp::min(this.buffer.len(), buf.remaining());
                    buf.put_slice(&this.buffer[..*this.position + to_copy]);
                    *this.position += to_copy;
                    return Poll::Ready(Ok(()));
                }
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }

        // no readers, we're done, i dont think this changes perf that much...
        if this.readers.len() == 0 {
            return Poll::Pending;
        }

        // read all
        let mut poll_bufs = vec![vec![0u8; 4096]; this.readers.len()];
        let mut poll_read_bufs = poll_bufs
            .iter_mut()
            .map(|buf| ReadBuf::new(buf))
            .collect::<Vec<_>>();
        let mut results = Vec::with_capacity(this.readers.len());
        results.extend(
            this.readers
                .values_mut()
                .zip(poll_read_bufs.iter_mut())
                .map(|(reader, read_buf)| Pin::new(reader).poll_read(cx, read_buf))
                .collect::<Vec<_>>(),
        );

        for (poll, read_buf) in results.into_iter().zip(poll_read_bufs) {
            match poll {
                Poll::Ready(Ok(())) => {
                    let filled = read_buf.filled();

                    if !filled.is_empty() {
                        *this.buffer = BytesMut::from(filled);
                        *this.position = 0;
                        let to_copy = std::cmp::min(this.buffer.len(), buf.remaining());
                        buf.put_slice(&this.buffer[..*this.position + to_copy]);
                        *this.position += to_copy;
                        return Poll::Ready(Ok(()));
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {}
            }
        }

        Poll::Pending
    }
}
