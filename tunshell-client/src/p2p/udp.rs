use crate::P2PConnection;
use crate::TunnelStream;
use anyhow::{Error, Result};
use async_trait::async_trait;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::TryFutureExt;
use log::*;
use std::hash::Hasher;
use std::io::Cursor;
use std::io::Result as IoResult;
use std::pin::Pin;
use std::task::{Context, Poll};
use thrussh::Tcp;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

const MAX_RECV_BUFF: u32 = 102400;
const RECV_TIMEOUT: u16 = 30;

pub struct UdpConnection {
    recv_sequence_number: u32,
    recv_message_buff: Vec<UdpMessage>,
    recv_buff: Vec<u8>,
    recv_state: Option<UdpRecvState>,
    send_sequence_number: u32,
    send_buff: Vec<UdpMessage>,
    sent_buff: Vec<UdpMessage>,
    send_state: Option<UdpSendState>,
    peer_ack_number: u32,
    peer_window: u32,
}

enum UdpRecvState {
    Idle(RecvHalf),
    Waiting(BoxFuture<'static, (RecvHalf, IoResult<Vec<u8>>)>),
}

enum UdpSendState {
    Idle(SendHalf),
    Waiting(BoxFuture<'static, (SendHalf, IoResult<usize>)>),
}

#[derive(Debug, PartialEq)]
struct UdpMessage {
    sequence_number: u32,
    ack_number: u32,
    window: u32,
    length: u16,
    checksum: u32,
    payload: Vec<u8>,
}

const UDP_MESSAGE_HEADER_SIZE: usize = 4 + 4 + 4 + 2 + 4;

impl UdpMessage {
    fn from(data: Vec<u8>) -> (IoResult<UdpMessage>, Vec<u8>) {
        if data.len() < UDP_MESSAGE_HEADER_SIZE {
            error!("received packet is too small: {}", data.len());
            return (
                Err(std::io::Error::from(std::io::ErrorKind::InvalidData)),
                data,
            );
        }

        let mut cursor = Cursor::new(data);

        let sequence_number = cursor.read_u32::<BigEndian>().unwrap();
        let ack_number = cursor.read_u32::<BigEndian>().unwrap();
        let window = cursor.read_u32::<BigEndian>().unwrap();
        let length = cursor.read_u16::<BigEndian>().unwrap();
        let checksum = cursor.read_u32::<BigEndian>().unwrap();

        let payload_start = cursor.position() as usize;
        let mut data = cursor.into_inner();
        data.drain(..payload_start);

        if data.len() < length as usize {
            error!(
                "received packet payload length mismatch, expected {} != actual {}",
                length,
                data.len()
            );
            return (
                Err(std::io::Error::from(std::io::ErrorKind::InvalidData)),
                data,
            );
        }

        let payload = data.drain(..length as usize).collect::<Vec<u8>>();

        let message = UdpMessage {
            sequence_number,
            ack_number,
            window,
            length,
            checksum,
            payload,
        };

        return (Ok(message), data);
    }

    fn to_vec(&self) -> Vec<u8> {
        use std::io::Write;

        let buff = Vec::with_capacity(UDP_MESSAGE_HEADER_SIZE + self.payload.len());

        let mut cursor = Cursor::new(buff);
        cursor.write_u32::<BigEndian>(self.sequence_number).unwrap();
        cursor.write_u32::<BigEndian>(self.ack_number).unwrap();
        cursor.write_u32::<BigEndian>(self.window).unwrap();
        cursor.write_u16::<BigEndian>(self.length).unwrap();
        cursor.write_u32::<BigEndian>(self.checksum).unwrap();
        cursor.write_all(self.payload.as_slice()).unwrap();

        cursor.into_inner()
    }

    fn is_checksum_valid(&self) -> bool {
        return self.checksum == self.calculate_checksum();
    }

    fn calculate_checksum(&self) -> u32 {
        let mut hasher = twox_hash::XxHash32::default();

        hasher.write_u32(self.sequence_number);
        hasher.write_u32(self.ack_number);
        hasher.write_u32(self.window);
        hasher.write_u16(self.length);
        hasher.write(self.payload.as_slice());

        return hasher.finish() as u32;
    }

    fn end_sequence_number(&self) -> u32 {
        // sequence number represents the index of the first byte
        // in the packet, the end sequence number is the index of
        // the last byte in the packet
        return self.sequence_number + self.length as u32;
    }

    fn overlaps(&self, other: &Self) -> bool {
        return self.sequence_number < other.end_sequence_number()
            && other.sequence_number < self.end_sequence_number();
    }
}

impl UdpConnection {
    fn new(socket: UdpSocket) -> Self {
        let (recv, send) = socket.split();

        Self {
            recv_sequence_number: 0,
            recv_message_buff: vec![],
            recv_buff: vec![],
            recv_state: Some(UdpRecvState::Idle(recv)),
            send_sequence_number: 0,
            send_buff: vec![],
            sent_buff: vec![],
            send_state: Some(UdpSendState::Idle(send)),
            peer_ack_number: 0,
            peer_window: MAX_RECV_BUFF,
        }
    }
}

impl UdpRecvState {
    fn poll(self, cx: &mut Context<'_>) -> (Self, Poll<IoResult<Vec<u8>>>) {
        let mut recv_future = match self {
            Self::Waiting(future) => future,
            Self::Idle(recv) => Self::await_for_next_recv(recv).boxed(),
        };

        match recv_future.as_mut().poll(cx) {
            Poll::Ready((socket, result)) => (Self::Idle(socket), Poll::Ready(result)),
            Poll::Pending => (Self::Waiting(recv_future), Poll::Pending),
        }
    }

    async fn await_for_next_recv(mut socket: RecvHalf) -> (RecvHalf, IoResult<Vec<u8>>) {
        let mut buff = [0u8; 1024];
        let result = tokio::select! {
            result = socket.recv(&mut buff) => result,
            _ = tokio::time::delay_for(std::time::Duration::from_secs(RECV_TIMEOUT as u64)) => IoResult::Err(std::io::Error::from(std::io::ErrorKind::TimedOut))
        };

        let result = match result {
            Ok(read) => Ok(Vec::from(&buff[..read])),
            Err(err) => Err(err),
        };

        (socket, result)
    }
}

impl UdpConnection {
    fn handle_recv(&mut self, mut data: Vec<u8>) -> IoResult<()> {
        while data.len() > 0 {
            let (result, remaining) = UdpMessage::from(data);
            data = remaining;

            let result = match result {
                Ok(message) => self.handle_recv_message(message),
                Err(err) => return Err(err),
            };

            if let Err(err) = result {
                return Err(err);
            }
        }

        Ok(())
    }

    fn handle_recv_message(&mut self, message: UdpMessage) -> IoResult<()> {
        if !message.is_checksum_valid() {
            error!(
                "corrupted message received, expected {} != actual {}",
                message.checksum,
                message.calculate_checksum()
            );

            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }

        if message.sequence_number < self.recv_sequence_number {
            error!("received message before current sequence number");
            return Err(std::io::Error::from(std::io::ErrorKind::Other));
        }

        if message.sequence_number > self.recv_sequence_number + MAX_RECV_BUFF {
            error!("received message outside of current window");
            return Err(std::io::Error::from(std::io::ErrorKind::Other));
        }

        if let Some(other) = self.recv_message_buff.iter().find(|i| message.overlaps(i)) {
            error!(
                "received message overlapping with other message {} - {}",
                other.sequence_number,
                other.end_sequence_number()
            );
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }

        if message.ack_number > self.peer_ack_number {
            self.peer_ack_number = message.ack_number;
        }

        // Insert the message in recv_message_buff while
        // maintaining ascending sequence numbers
        let insert_at = self
            .recv_message_buff
            .iter()
            .take_while(|i| i.sequence_number < message.sequence_number)
            .count();
        let is_last = insert_at == self.recv_message_buff.len();

        if is_last {
            self.peer_window = message.window;
        }

        self.recv_message_buff.insert(insert_at, message);
        self.process_recv_messages();
        Ok(())
    }

    fn process_recv_messages(&mut self) {
        let mut consumed = 0;

        for message in self.recv_message_buff.iter() {
            if self.recv_sequence_number != message.sequence_number {
                break;
            }

            self.recv_buff.extend_from_slice(&message.payload[..]);
            self.recv_sequence_number = message.end_sequence_number();
            consumed += 1;
            info!("successfully received {} bytes", message.length);
        }

        self.recv_message_buff.drain(..consumed);
        self.send_ack_update();
    }
}

impl AsyncRead for UdpConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<IoResult<usize>> {
        assert!(buff.len() > 0);

        loop {
            let available_to_read = std::cmp::min(buff.len(), self.recv_buff.len());

            if available_to_read > 0 {
                buff[..available_to_read].copy_from_slice(&self.recv_buff[..available_to_read]);
                self.recv_buff.drain(..available_to_read);

                return Poll::Ready(Ok(available_to_read));
            }

            let (new_state, result) = self.recv_state.take().unwrap().poll(cx);
            self.recv_state.replace(new_state);

            let data = match result {
                Poll::Ready(Ok(data)) => data,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            };

            match self.handle_recv(data) {
                Ok(_) => {}
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}

impl UdpConnection {
    fn send_ack_update(&mut self) {
        // TODO
    }
}

impl AsyncWrite for UdpConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<IoResult<usize>> {
        todo!();
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        todo!();
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        todo!();
    }
}

/// A requirement of using this a underlying stream for thrussh
impl Tcp for UdpConnection {}

impl TunnelStream for UdpConnection {}

#[async_trait]
impl P2PConnection for UdpConnection {
    async fn connect(
        peer_info: &PeerJoinedPayload,
        connection_info: &AttemptDirectConnectPayload,
    ) -> Result<Self> {
        info!(
            "Attempting to connect to {} via UDP",
            peer_info.peer_ip_address
        );

        let mut socket =
            UdpSocket::bind("0.0.0.0:".to_owned() + &connection_info.self_listen_port.to_string())
                .await?;

        tokio::select! {
            result = negotiate_connection(&mut socket, peer_info, connection_info) => return match result {
                Ok(_) => Ok(UdpConnection::new(socket)),
                Err(err) => Err(err)
            },
            _ = tokio::time::delay_for(std::time::Duration::from_secs(3)) => {
                info!("timed out while attempting UDP connection");
            }
        };

        Err(Error::msg("Direct UDP connection timed out"))
    }
}

// TODO: define proper connection negotiation, ip validation etc
async fn negotiate_connection(
    mut socket: &mut UdpSocket,
    peer_info: &PeerJoinedPayload,
    connection_info: &AttemptDirectConnectPayload,
) -> Result<()> {
    let peer_addr =
        peer_info.peer_ip_address.clone() + ":" + &connection_info.peer_listen_port.to_string();

    info!("Attempting to connect to {}", peer_addr);
    send_hello(&mut socket, Some(peer_addr)).await?;
    let peer_addr = wait_for_hello(&mut socket).await?;

    info!("Connected to {}", peer_addr);
    socket.connect(peer_addr).await?;

    send_hello(&mut socket, Option::None::<String>).await?;
    wait_for_hello(&mut socket).await?;
    info!("Connection confirmed {}", peer_addr);

    Ok(())
}

async fn send_hello(
    socket: &mut UdpSocket,
    target: Option<impl tokio::net::ToSocketAddrs>,
) -> Result<()> {
    match target {
        Some(addr) => socket.send_to("hello".as_bytes(), addr).await?,
        None => socket.send("hello".as_bytes()).await?,
    };

    Ok(())
}

async fn wait_for_hello(socket: &mut UdpSocket) -> Result<std::net::SocketAddr> {
    let mut buff = [0; 1024];
    let (read, peer_addr) = socket
        .recv_from(&mut buff)
        .await
        .map_err(|err| Error::new(err))?;

    if String::from_utf8(buff[..read].to_vec())? == "hello" {
        Ok(peer_addr)
    } else {
        Err(Error::msg("Unexpected hello message"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::TryFutureExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_parse_udp_message() {
        let raw_data = vec![
            0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1, 2, 3, 4, 5, 6,
        ];

        let (message, remaining) = UdpMessage::from(raw_data);
        let message = message.unwrap();

        assert_eq!(message.sequence_number, 1);
        assert_eq!(message.ack_number, 2);
        assert_eq!(message.window, 3);
        assert_eq!(message.length, 4);
        assert_eq!(message.checksum, 5);
        assert_eq!(message.payload, vec![1, 2, 3, 4]);
        assert_eq!(remaining, vec![5, 6]);
    }

    #[test]
    fn test_parse_udp_message_too_short() {
        let raw_data = vec![1, 2, 3, 4];

        let (message, remaining) = UdpMessage::from(raw_data);

        assert!(message.is_err());
        assert_eq!(remaining, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_parse_udp_message_not_enough_payload() {
        let raw_data = vec![0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1];

        let (message, _) = UdpMessage::from(raw_data);

        assert!(message.is_err());
    }

    #[test]
    fn test_udp_message_calculate_hash() {
        let message = UdpMessage {
            sequence_number: 0,
            ack_number: 1,
            window: 2,
            length: 3,
            checksum: 0,
            payload: vec![1, 2, 3, 4],
        };

        assert_eq!(message.calculate_checksum(), 3014949120);
    }
    #[test]
    fn test_udp_message_to_vec() {
        let message = UdpMessage {
            sequence_number: 0,
            ack_number: 1,
            window: 2,
            length: 3,
            checksum: 4,
            payload: vec![1, 2, 3],
        };

        let result = message.to_vec();

        assert_eq!(
            result,
            vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 3, 0, 0, 0, 4, 1, 2, 3]
        );
    }

    #[test]
    fn test_udp_message_to_vec_then_parse() {
        let message = UdpMessage {
            sequence_number: 12345765,
            ack_number: 46547747,
            window: 67653435,
            length: 23,
            checksum: 44365478,
            payload: vec![
                1, 2, 3, 54, 5, 6, 54, 65, 6, 5, 7, 65, 76, 87, 86, 7, 8, 7, 98, 7, 89, 79, 2,
            ],
        };

        let (parsed_message, remaining) = UdpMessage::from(message.to_vec());
        let parsed_message = parsed_message.unwrap();

        assert_eq!(parsed_message, message);
        assert_eq!(remaining, Vec::<u8>::new());
    }

    #[test]
    fn test_udp_message_end_sequence_number() {
        let message = UdpMessage {
            sequence_number: 100,
            ack_number: 0,
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };

        assert_eq!(message.end_sequence_number(), 110);
    }

    #[test]
    fn test_udp_message_overlap() {
        let message1 = UdpMessage {
            sequence_number: 100,
            ack_number: 0,
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };
        let message2 = UdpMessage {
            sequence_number: 110,
            ack_number: 0,
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };
        let message3 = UdpMessage {
            sequence_number: 105,
            ack_number: 0,
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };

        assert!(message1.overlaps(&message1));
        assert!(!message1.overlaps(&message2));
        assert!(!message2.overlaps(&message1));
        assert!(message1.overlaps(&message3));
        assert!(message3.overlaps(&message1));
        assert!(message2.overlaps(&message3));
        assert!(message3.overlaps(&message2));
    }

    static mut UDP_PORT_NUMBER: u16 = 22660;

    async fn init_new_udp_connection_writable() -> (UdpConnection, UdpSocket) {
        let (port1, port2) = unsafe {
            UDP_PORT_NUMBER += 2;
            (UDP_PORT_NUMBER, UDP_PORT_NUMBER - 1)
        };

        let connection = UdpConnection::new(
            UdpSocket::bind("0.0.0.0:".to_owned() + &port1.to_string())
                .await
                .unwrap(),
        );
        let write_socket = UdpSocket::bind("0.0.0.0:".to_owned() + &port2.to_string())
            .await
            .unwrap();
        write_socket
            .connect("127.0.0.1:".to_owned() + &port1.to_string())
            .await
            .unwrap();

        return (connection, write_socket);
    }

    #[test]
    fn test_connection_recv_valid_message() {
        Runtime::new().unwrap().block_on(async {
            let (mut connection, mut write_socket) = init_new_udp_connection_writable().await;

            let mut message = UdpMessage {
                sequence_number: 0,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![1, 2, 3],
            };
            message.checksum = message.calculate_checksum();

            write_socket
                .send(message.to_vec().as_slice())
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = connection.read(&mut buff).await.unwrap();

            assert_eq!(buff[..read], [1, 2, 3]);
            assert_eq!(connection.recv_buff, Vec::<u8>::new());
            assert_eq!(connection.recv_message_buff, Vec::<UdpMessage>::new());
            assert_eq!(connection.recv_sequence_number, 3);
            assert_eq!(connection.peer_ack_number, 1);
            assert_eq!(connection.peer_window, 2);
        });
    }

    #[test]
    fn test_connection_recv_out_of_order_messages() {
        Runtime::new().unwrap().block_on(async {
            let (mut connection, mut write_socket) = init_new_udp_connection_writable().await;

            let mut message1 = UdpMessage {
                sequence_number: 3,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![4, 5, 6],
            };
            message1.checksum = message1.calculate_checksum();

            write_socket
                .send(message1.to_vec().as_slice())
                .await
                .unwrap();

            let mut message2 = UdpMessage {
                sequence_number: 0,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![1, 2, 3],
            };
            message2.checksum = message2.calculate_checksum();

            write_socket
                .send(message2.to_vec().as_slice())
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = connection.read(&mut buff).await.unwrap();

            assert_eq!(buff[..read], [1, 2, 3, 4, 5, 6]);
            assert_eq!(connection.recv_buff, Vec::<u8>::new());
            assert_eq!(connection.recv_message_buff, Vec::<UdpMessage>::new());
            assert_eq!(connection.recv_sequence_number, 6);
            assert_eq!(connection.peer_ack_number, 1);
            assert_eq!(connection.peer_window, 2);
        });
    }

    #[test]
    fn test_connect_timeout() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };
            let connection1 = UdpConnection::connect(
                &peer_info,
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22554,
                    self_listen_port: 22555,
                },
            );

            let (_, result) = futures::join!(
                tokio::time::delay_for(std::time::Duration::from_secs(5)),
                connection1
            );

            assert!(result.is_err());
        });
    }

    #[test]
    fn test_connect_simultaneous_open() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };

            let connection1 = UdpConnection::connect(
                &peer_info,
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22664,
                    self_listen_port: 22665,
                },
            );

            let connection2 = UdpConnection::connect(
                &peer_info,
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22665,
                    self_listen_port: 22664,
                },
            );

            let (mut connection1, mut connection2) =
                futures::try_join!(connection1, connection2).expect("failed to connect");

            // connection1.write("hello from 1".as_bytes()).await.unwrap();

            // let mut buff = [0; 1024];
            // let read = connection2.read(&mut buff).await.unwrap();

            // assert_eq!(
            //     String::from_utf8(buff[..read].to_vec()).unwrap(),
            //     "hello from 1"
            // );

            // connection2.write("hello from 2".as_bytes()).await.unwrap();

            // let mut buff = [0; 1024];
            // let read = connection1.read(&mut buff).await.unwrap();

            // assert_eq!(
            //     String::from_utf8(buff[..read].to_vec()).unwrap(),
            //     "hello from 2"
            // );
        });
    }
}
