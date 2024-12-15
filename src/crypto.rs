use chacha20::{
    ChaCha20,
    cipher::{KeyIvInit, StreamCipher},
};
use rand::{RngCore, rngs::OsRng};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

pub struct EncryptedPipe<S> {
    inner: S,
    cipher: ChaCha20,
}

impl<S> EncryptedPipe<S> {
    pub fn new(stream: S, key: &[u8; 32], nonce: &[u8; 12]) -> Self {
        Self {
            inner: stream,
            cipher: ChaCha20::new(key.into(), nonce.into()),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for EncryptedPipe<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Read from inner stream
        let mut temp_buf = vec![0u8; 1024];
        let mut inner_buf = ReadBuf::new(&mut temp_buf);
        match Pin::new(&mut self.inner).poll_read(cx, &mut inner_buf) {
            Poll::Ready(Ok(())) => {
                let filled = inner_buf.filled_mut();
                if !filled.is_empty() {
                    // Encrypt the data in place
                    self.cipher.apply_keystream(filled);
                    // Copy to output buffer
                    buf.put_slice(filled);
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for EncryptedPipe<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Create a mutable copy of the data
        let mut data = buf.to_vec();
        // Encrypt in place
        self.cipher.apply_keystream(&mut data);
        // Write encrypted data
        Pin::new(&mut self.inner).poll_write(cx, &data)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub fn encrypted_duplex(
    key: &[u8; 32],
) -> (
    EncryptedPipe<tokio::io::DuplexStream>,
    EncryptedPipe<tokio::io::DuplexStream>,
) {
    encrypted_duplex_custom(key, 64)
}

pub fn encrypted_duplex_custom(
    key: &[u8; 32],
    max_buf_size: usize,
) -> (
    EncryptedPipe<tokio::io::DuplexStream>,
    EncryptedPipe<tokio::io::DuplexStream>,
) {
    let (reader, writer) = tokio::io::duplex(max_buf_size);

    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);

    let reader = EncryptedPipe::new(reader, key, &nonce);
    let writer = EncryptedPipe::new(writer, key, &nonce);

    (reader, writer)
}
