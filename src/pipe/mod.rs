use tokio::io::{AsyncRead, AsyncWrite};

mod mspc;
mod reader;
mod writer;

// this function assumes that the stream i/o is already linked
pub fn split<S>(
    stream: S,
    buffer_size: Option<usize>,
) -> (
    PipeReader<tokio::io::ReadHalf<S>>,
    PipeWriter<tokio::io::WriteHalf<S>>,
)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (r, w) = tokio::io::split(stream);
    (
        PipeReader::new(r, None, buffer_size),
        PipeWriter::new(w, None, buffer_size),
    )
}

pub use mspc::{ReceiverReader, SenderWriter};
pub use reader::PipeReader;
pub use writer::PipeWriter;
