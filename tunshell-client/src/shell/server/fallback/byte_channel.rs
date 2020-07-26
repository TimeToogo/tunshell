use io::Write;
use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use tokio::io::AsyncRead;

//// An in-memory buffer with a sync write and an async read half
pub(super) struct ByteChannel {
    buff: Vec<u8>,
    read_wakers: Vec<Waker>,
    shutdown: bool,
}

impl ByteChannel {
    pub(super) fn new() -> Self {
        Self {
            buff: vec![],
            read_wakers: vec![],
            shutdown: false,
        }
    }
}

impl AsyncRead for ByteChannel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if self.buff.len() == 0 {
            if self.shutdown {
                return Poll::Ready(Ok(0));
            }

            self.read_wakers.push(cx.waker().clone());
            return Poll::Pending;
        }

        let len = cmp::min(buf.len(), self.buff.len());
        &buf[..len].copy_from_slice(self.buff.drain(..len).collect::<Vec<u8>>().as_slice());
        Poll::Ready(Ok(len))
    }
}

impl Write for ByteChannel {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        if self.shutdown {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }

        self.buff.extend_from_slice(buf);

        for i in self.read_wakers.drain(..) {
            i.wake();
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ByteChannel {
    pub(super) fn shutdown(&mut self) {
        self.shutdown = true;

        for i in self.read_wakers.drain(..) {
            i.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::{io::AsyncReadExt, runtime::Runtime, time::timeout};

    #[test]
    fn test_new() {
        let stream = ByteChannel::new();

        assert_eq!(stream.buff, Vec::<u8>::new());
        assert_eq!(stream.read_wakers.len(), 0);
    }

    #[test]
    fn test_write() {
        let mut stream = ByteChannel::new();

        stream.write(&[1, 2, 3]).unwrap();

        assert_eq!(stream.buff, vec![1, 2, 3]);
    }

    #[test]
    fn test_write_then_read() {
        let mut stream = ByteChannel::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            stream.write(&[1, 2, 3]).unwrap();

            let mut read_buff = [0u8; 1024];
            let read = stream.read(&mut read_buff).await.unwrap();

            (read, read_buff)
        });

        assert_eq!(read, 3);
        assert_eq!(&read_buff[..3], &[1, 2, 3]);

        assert_eq!(stream.buff, Vec::<u8>::new());
    }

    // #[test]
    // fn test_read_then_write() {
    //     let mut stream = Arc::new(Mutex::new(ByteChannel::new()));

    //     let (read, read_buff) = Runtime::new().unwrap().block_on(async {
    //         let mut read_stream = Arc::clone(&stream);

    //         let read_task = tokio::spawn(async move {
    //             let read_stream = read_stream.lock().unwrap();
    //             let mut read_buff = [0u8; 1024];
    //             let read = read_stream.read(&mut read_buff).await.unwrap();

    //             (read, read_buff)
    //         });

    //         let stream = stream.lock().unwrap();
    //         stream.write(&[1, 2, 3]).unwrap();

    //         read_task.await.unwrap()
    //     });

    //     assert_eq!(read, 3);
    //     assert_eq!(&read_buff[..3], &[1, 2, 3]);

    //     let stream = stream.lock().unwrap();
    //     assert_eq!(stream.buff, Vec::<u8>::new());
    // }

    #[test]
    fn test_read_empty() {
        let mut stream = ByteChannel::new();

        Runtime::new().unwrap().block_on(async {
            let mut read_buff = [0u8; 1024];

            timeout(Duration::from_millis(100), stream.read(&mut read_buff))
                .await
                .expect_err("should await until there is data written");
        });
    }

    #[test]
    fn test_read_after_close() {
        let mut stream = ByteChannel::new();

        Runtime::new().unwrap().block_on(async {
            stream.shutdown();

            assert_eq!(stream.read(&mut [0u8; 1024]).await.unwrap(), 0);
        });
    }

    #[test]
    fn test_write_after_close() {
        let mut stream = ByteChannel::new();

        Runtime::new().unwrap().block_on(async {
            stream.shutdown();

            stream
                .write(&mut [0u8; 1024])
                .expect_err("should return error if writing after closing stream");
        });
    }
}
