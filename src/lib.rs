//! Encrypted pipe implementation for streaming data
//!
//! This crate provides a simple way to create encrypted streams
//! that can be used for streaming large files.

// mod crypto;
extern crate chacha20;
extern crate futures;
extern crate pin_project;
extern crate tokio;

mod crypto;
mod pipe;

pub use crypto::{EncryptedPipe, encrypted_duplex};
pub use pipe::{PipeReader, PipeWriter, split};

pub mod prelude {
    pub use crate::{EncryptedPipe, encrypted_duplex};
    pub use crate::{PipeReader, PipeWriter, split};
}
