use super::ByteChannel;
use io::Write;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncRead;

const CR: u8 = 0x0D;
const LF: u8 = 0x0A;

/// Output byte stream for the fallback terminal
/// Performs conversion of LF to CRLF line endings.
pub(super) struct OutputStream {
    inner: ByteChannel,
    last_byte: Option<u8>,
}

impl OutputStream {
    pub(super) fn new() -> Self {
        Self {
            inner: ByteChannel::new(),
            last_byte: None,
        }
    }

    pub(super) fn shutdown(&mut self) {
        self.inner.shutdown();
    }
}

impl AsyncRead for OutputStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Write for OutputStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        // Convert LF to CRLF
        let mut written = 0;

        if buf[0] == LF && self.last_byte.unwrap_or(0) != CR {
            self.inner.write_all(&[CR, LF])?;
            written += 1;
        }

        for i in written..buf.len() {
            if i > 0 && buf[i] == LF && buf[i - 1] != CR {
                self.inner.write_all(&buf[written..i])?;
                self.inner.write_all(&[CR, LF])?;
                written = i + 1;
            }
        }

        self.inner.write_all(&buf[written..])?;
        self.last_byte = Some(buf[buf.len() - 1]);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncReadExt, runtime::Runtime};

    #[test]
    fn test_new() {
        let stream = OutputStream::new();

        assert_eq!(stream.last_byte, None);
    }

    #[test]
    fn test_write_then_read() {
        let mut stream = OutputStream::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            stream.write(&[1, 2, 3]).unwrap();

            let mut read_buff = [0u8; 1024];
            let read = stream.read(&mut read_buff).await.unwrap();

            (read, read_buff)
        });

        assert_eq!(read, 3);
        assert_eq!(&read_buff[..3], &[1, 2, 3]);
    }

    #[test]
    fn test_write_then_read_replaces_lf_to_crlf() {
        let mut stream = OutputStream::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            stream.write(&[LF, 2, 3, LF, 5, 6, CR, LF, LF]).unwrap();

            let mut read_buff = [0u8; 1024];
            let read = stream.read(&mut read_buff).await.unwrap();

            (read, read_buff)
        });

        assert_eq!(
            &read_buff[..read],
            &[CR, LF, 2, 3, CR, LF, 5, 6, CR, LF, CR, LF]
        );
    }

    #[test]
    fn test_write_then_read_replaces_lf_to_crlf_over_multiple_writes() {
        let mut stream = OutputStream::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            stream.write(&[LF]).unwrap();
            stream.write(&[1, 2, 3]).unwrap();
            stream.write(&[LF, 4, 5, 6, CR]).unwrap();
            stream.write(&[LF, 7, 8, 9]).unwrap();

            let mut read_buff = [0u8; 1024];
            let read = stream.read(&mut read_buff).await.unwrap();

            (read, read_buff)
        });

        assert_eq!(
            &read_buff[..read],
            &[CR, LF, 1, 2, 3, CR, LF, 4, 5, 6, CR, LF, 7, 8, 9]
        );
    }
}
