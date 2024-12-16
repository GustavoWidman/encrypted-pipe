//! Encrypted pipe implementation for streaming data
//!
//! This crate provides a simple way to create encrypted streams
//! that can be used for streaming large files.

// mod crypto;
extern crate anyhow;
extern crate chacha20;
extern crate futures;
extern crate pin_project;
extern crate tokio;
extern crate tokio_stream;
extern crate tokio_util;

use tokio::io::{AsyncRead, AsyncWrite};

mod reader;
mod writer;

// this function assumes that the stream i/o is already linked
pub fn split<S>(
    stream: S,
    buffer_size: Option<usize>,
) -> (
    MultiReader<tokio::io::ReadHalf<S>>,
    MultiWriter<tokio::io::WriteHalf<S>>,
)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (r, w) = tokio::io::split(stream);
    (
        MultiReader::new(r, None, buffer_size),
        MultiWriter::new(w, None, buffer_size),
    )
}

pub use reader::MultiReader;
pub use writer::MultiWriter;

pub mod prelude {
    pub use crate::{MultiReader, MultiWriter, split};
}
