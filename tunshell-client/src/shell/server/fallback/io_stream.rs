use io::Write;
use std::{
    cmp, io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use tokio::io::AsyncRead;

struct Inner {
    buff: Vec<u8>,
    read_wakers: Vec<Waker>,
    shutdown: bool,
}

/// An in-memory buffer with a sync write and an async read half
pub(super) struct IoStream {
    state: Arc<Mutex<Inner>>,
}

impl IoStream {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Inner {
                buff: vec![],
                read_wakers: vec![],
                shutdown: false,
            })),
        }
    }
}

impl AsyncRead for IoStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.state.lock().unwrap();

        if state.shutdown {
            return Poll::Ready(Ok(0));
        }

        if state.buff.len() == 0 {
            state.read_wakers.push(cx.waker().clone());
            return Poll::Pending;
        }

        let len = cmp::min(buf.len(), state.buff.len());
        &buf[..len].copy_from_slice(state.buff.drain(..len).collect::<Vec<u8>>().as_slice());
        Poll::Ready(Ok(len))
    }
}

impl Write for IoStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.state.lock().unwrap();

        if state.shutdown {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }

        state.buff.extend_from_slice(buf);

        for i in state.read_wakers.drain(..) {
            i.wake();
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl IoStream {
    pub(super) fn process_buff<R>(&mut self, func: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        let mut state = self.state.lock().unwrap();
        func(&mut state.buff)
    }

    #[allow(dead_code)]
    pub(super) fn shutdown(&mut self) {
        let mut state = self.state.lock().unwrap();

        state.shutdown = true;

        for i in state.read_wakers.drain(..) {
            i.wake();
        }
    }
}

impl Clone for IoStream {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
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
        let buff = IoStream::new();

        let state = buff.state.lock().unwrap();
        assert_eq!(state.buff, Vec::<u8>::new());
        assert_eq!(state.read_wakers.len(), 0);
    }

    #[test]
    fn test_write() {
        let mut stream = IoStream::new();

        stream.write(&[1, 2, 3]).unwrap();

        let state = stream.state.lock().unwrap();
        assert_eq!(state.buff, vec![1, 2, 3]);
    }

    #[test]
    fn test_write_then_read() {
        let mut stream = IoStream::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            stream.write(&[1, 2, 3]).unwrap();

            let mut read_buff = [0u8; 1024];
            let read = stream.read(&mut read_buff).await.unwrap();

            (read, read_buff)
        });

        assert_eq!(read, 3);
        assert_eq!(&read_buff[..3], &[1, 2, 3]);

        let state = stream.state.lock().unwrap();
        assert_eq!(state.buff, Vec::<u8>::new());
    }

    #[test]
    fn test_read_then_write() {
        let mut stream = IoStream::new();

        let (read, read_buff) = Runtime::new().unwrap().block_on(async {
            let mut read_stream = stream.clone();

            let read_task = tokio::spawn(async move {
                let mut read_buff = [0u8; 1024];
                let read = read_stream.read(&mut read_buff).await.unwrap();

                (read, read_buff)
            });

            stream.write(&[1, 2, 3]).unwrap();

            read_task.await.unwrap()
        });

        assert_eq!(read, 3);
        assert_eq!(&read_buff[..3], &[1, 2, 3]);

        let state = stream.state.lock().unwrap();
        assert_eq!(state.buff, Vec::<u8>::new());
    }

    #[test]
    fn test_read_empty() {
        let mut stream = IoStream::new();

        Runtime::new().unwrap().block_on(async {
            let mut read_buff = [0u8; 1024];

            timeout(Duration::from_millis(100), stream.read(&mut read_buff))
                .await
                .expect_err("should await until there is data written");
        });
    }

    #[test]
    fn test_read_after_close() {
        let mut stream = IoStream::new();

        Runtime::new().unwrap().block_on(async {
            stream.shutdown();

            assert_eq!(stream.read(&mut [0u8; 1024]).await.unwrap(), 0);
        });
    }

    #[test]
    fn test_write_after_close() {
        let mut stream = IoStream::new();

        Runtime::new().unwrap().block_on(async {
            stream.shutdown();

            stream
                .write(&mut [0u8; 1024])
                .expect_err("should return error if writing after closing stream");
        });
    }
}
