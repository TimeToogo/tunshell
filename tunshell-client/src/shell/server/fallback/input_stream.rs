use futures::Stream;
use io::Write;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
};

// Ascii escape code chars
const ESC: u8 = 27;
const CR: u8 = 13;
const DEL: u8 = 127;
const EOT: u8 = 3;

/// Input byte stream for the fallback terminal
/// Performs limited tokenisation of vt100 escape sequences
/// @see https://vt100.net/docs/tp83/appendixb.html
pub(super) struct InputStream {
    buff: Vec<u8>,
    tokens: Vec<Token>,
    next_wakers: Vec<Waker>,
    shutdown: bool,
}

#[derive(PartialEq, Clone, Debug)]
pub(super) enum Token {
    Bytes(Vec<u8>),
    Enter,
    UpArrow,
    LeftArrow,
    RightArrow,
    DownArrow,
    Backspace,
    ControlC,
}

impl InputStream {
    pub(super) fn new() -> Self {
        Self {
            buff: vec![],
            tokens: vec![],
            next_wakers: vec![],
            shutdown: false,
        }
    }

    fn parse_buff(&mut self) {
        let mut bytes_start = 0;
        let mut bytes_len = 0;
        let mut i = 0;

        while i < self.buff.len() {
            let token = match (self.buff[i], self.buff.get(i + 1), self.buff.get(i + 2)) {
                (ESC, Some(b'['), Some(b'A')) => Some((Token::UpArrow, 3)),
                (ESC, Some(b'['), Some(b'B')) => Some((Token::DownArrow, 3)),
                (ESC, Some(b'['), Some(b'C')) => Some((Token::RightArrow, 3)),
                (ESC, Some(b'['), Some(b'D')) => Some((Token::LeftArrow, 3)),
                (ESC, _, None) => break,
                (CR, _, _) => Some((Token::Enter, 1)),
                (DEL, _, _) => Some((Token::Backspace, 1)),
                (EOT, _, _) => Some((Token::ControlC, 1)),
                _ => None,
            };

            if let Some((token, consumed)) = token {
                if bytes_len > 0 {
                    self.push_token(Token::Bytes(
                        self.buff[bytes_start..(bytes_start + bytes_len)].to_vec(),
                    ));
                }

                i += consumed;

                self.push_token(token);
                bytes_start = i;
                bytes_len = 0;
            } else {
                i += 1;
                bytes_len += 1;
            }
        }

        if bytes_len > 0 {
            self.push_token(Token::Bytes(
                self.buff[bytes_start..(bytes_start + bytes_len)].to_vec(),
            ));
        }

        self.buff.drain(..(bytes_start + bytes_len));
    }

    fn push_token(&mut self, token: Token) {
        self.tokens.push(token);

        for waker in self.next_wakers.drain(..) {
            waker.wake();
        }
    }

    pub(super) fn shutdown(&mut self) {
        self.shutdown = true;

        for i in self.next_wakers.drain(..) {
            i.wake();
        }
    }
}

impl Stream for InputStream {
    type Item = Token;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.tokens.len() == 0 && self.buff.len() > 0 {
            self.parse_buff();
        }

        if self.tokens.len() > 0 {
            return Poll::Ready(Some(self.tokens.remove(0)));
        }

        if self.shutdown {
            return Poll::Ready(None);
        }

        self.next_wakers.push(cx.waker().clone());
        Poll::Pending
    }
}

impl Write for InputStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        self.buff.extend_from_slice(buf);

        for i in self.next_wakers.drain(..) {
            i.wake();
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Token {
    pub(super) fn to_bytes(&self) -> &[u8] {
        match self {
            Token::Bytes(bytes) => bytes.as_slice(),
            Token::Enter => &[CR],
            Token::UpArrow => &[ESC, b'[', b'A'],
            Token::LeftArrow => &[ESC, b'[', b'D'],
            Token::RightArrow => &[ESC, b'[', b'C'],
            Token::DownArrow => &[ESC, b'[', b'B'],
            Token::Backspace => &[DEL],
            Token::ControlC => &[EOT],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tokio::runtime::Runtime;

    async fn drain_tokens(stream: &mut InputStream) -> Vec<Token> {
        futures::future::poll_fn(move |cx| {
            let mut tokens = vec![];

            while let Poll::Ready(Some(token)) = stream.poll_next_unpin(cx) {
                tokens.push(token);
            }

            Poll::Ready(tokens)
        })
        .await
    }

    #[test]
    fn test_new() {
        let stream = InputStream::new();

        assert_eq!(stream.buff, Vec::<u8>::new());
    }

    #[test]
    fn test_parsing_string_bytes() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write("hello world".as_bytes()).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(
            tokens,
            vec![Token::Bytes("hello world".as_bytes().to_vec())]
        );
    }

    #[test]
    fn test_parsing_string_bytes_multiple_writes() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write("hello ".as_bytes()).unwrap();
            stream.write("world".as_bytes()).unwrap();
            stream.write("!!".as_bytes()).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(
            tokens,
            vec![Token::Bytes("hello world!!".as_bytes().to_vec())]
        );
    }

    #[test]
    fn test_parsing_up_arrow() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[ESC, b'[', b'A']).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::UpArrow]);
    }

    #[test]
    fn test_parsing_down_arrow() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[ESC, b'[', b'B']).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::DownArrow]);
    }

    #[test]
    fn test_parsing_right_arrow() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[ESC, b'[', b'C']).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::RightArrow]);
    }

    #[test]
    fn test_parsing_left_arrow() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[ESC, b'[', b'D']).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::LeftArrow]);
    }

    #[test]
    fn test_parsing_del() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[DEL]).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::Backspace]);
    }

    #[test]
    fn test_parsing_control_c() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[EOT]).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::ControlC]);
    }

    #[test]
    fn test_parsing_enter() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[CR]).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![Token::Enter]);
    }

    #[test]
    fn test_parsing_partial_escape_sequence() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[ESC, b'[']).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(tokens, vec![]);
        assert_eq!(stream.buff, vec![ESC, b'[']);
    }

    #[test]
    fn test_parsing_multiple_tokens() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream
                .write(&[
                    b'h', b'i', ESC, b'[', b'A', b't', b'e', b's', b't', DEL, ESC, b'[', b'C', b'a',
                ])
                .unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(
            tokens,
            vec![
                Token::Bytes("hi".as_bytes().to_vec()),
                Token::UpArrow,
                Token::Bytes("test".as_bytes().to_vec()),
                Token::Backspace,
                Token::RightArrow,
                Token::Bytes([b'a'].to_vec())
            ]
        );
    }

    #[test]
    fn test_parsing_escape_sequence_over_multiple_writes() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write(&[b'h', b'i', ESC]).unwrap();

            let tokens = drain_tokens(&mut stream).await;

            stream
                .write(&[b'[', b'A', b't', b'e', b's', b't', DEL, ESC, b'['])
                .unwrap();

            let tokens = [tokens, drain_tokens(&mut stream).await].concat();

            stream.write(&[b'C', b'a']).unwrap();

            let tokens = [tokens, drain_tokens(&mut stream).await].concat();

            tokens
        });

        assert_eq!(
            tokens,
            vec![
                Token::Bytes("hi".as_bytes().to_vec()),
                Token::UpArrow,
                Token::Bytes("test".as_bytes().to_vec()),
                Token::Backspace,
                Token::RightArrow,
                Token::Bytes([b'a'].to_vec())
            ]
        );
    }

    #[test]
    fn test_parsing_multiple_lines() {
        let mut stream = InputStream::new();

        let tokens = Runtime::new().unwrap().block_on(async {
            stream.write("echo test\rexit\r".as_bytes()).unwrap();

            drain_tokens(&mut stream).await
        });

        assert_eq!(
            tokens,
            vec![
                Token::Bytes("echo test".as_bytes().to_vec()),
                Token::Enter,
                Token::Bytes("exit".as_bytes().to_vec()),
                Token::Enter,
            ]
        );
    }
}
