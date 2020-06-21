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
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use thrussh::Tcp;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

const MAX_RECV_BUFF: u32 = 102400;
const RECV_TIMEOUT: u16 = 10;
const SEND_TIMEOUT: u16 = 10;
const MAX_PACKET_SIZE: usize = 576;
const UDP_MESSAGE_HEADER_SIZE: usize = 4 + 4 + 4 + 2 + 4;
const DEFAULT_RTT_ESTIMATE: u16 = 3000;
const KEEP_ALIVE_INTERVAL: u32 = 30000;

pub struct UdpConnection {
    handle: Arc<Mutex<ConnectionHandle>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

#[derive(PartialEq)]
enum State {
    Active,
    Closed,
}

struct ConnectionHandle {
    state: State,
    recv_sequence_number: u32,
    recv_message_buff: Vec<UdpMessage>,
    recv_buff: Vec<u8>,
    send_sequence_number: u32,
    send_buff: Vec<u8>,
    sending_packet: Option<UdpMessage>,
    sent_packets: Vec<SentPacket>,
    last_send_at: Option<Instant>,
    peer_ack_number: u32,
    peer_window: u32,
    send_zero_window_waker: Option<Waker>,
    rtt_estimate: Duration,
    keep_alive_interval: Duration,
    recv_state: Option<UdpRecvState>,
    send_state: Option<UdpSendState>,
}

#[derive(Debug, PartialEq)]
struct SentPacket {
    packet: UdpMessage,
    sent_at: Instant,
}

enum UdpRecvState {
    Idle(RecvHalf),
    Waiting(BoxFuture<'static, (RecvHalf, IoResult<Vec<u8>>)>),
}

enum UdpSendState {
    Idle(SendHalf),
    Waiting(BoxFuture<'static, (SendHalf, IoResult<usize>)>),
}

#[derive(Debug, PartialEq, Clone)]
struct UdpMessage {
    sequence_number: u32,
    ack_number: u32,
    window: u32,
    length: u16,
    checksum: u32,
    payload: Vec<u8>,
}

impl UdpMessage {
    fn parse(data: Vec<u8>) -> (IoResult<UdpMessage>, Vec<u8>) {
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

    fn create(
        sequence_number: u32,
        ack_number: u32,
        window: u32,
        peer_window: u32,
        mut buff: Vec<u8>,
    ) -> (UdpMessage, Vec<u8>) {
        let length = std::cmp::min(buff.len(), MAX_PACKET_SIZE - UDP_MESSAGE_HEADER_SIZE) as u16;
        let length = std::cmp::min(length as u32, peer_window) as u16;
        let payload = buff.drain(..(length as usize)).collect::<Vec<u8>>();

        let mut message = UdpMessage {
            sequence_number,
            ack_number,
            window,
            length,
            checksum: 0,
            payload,
        };

        message.checksum = message.calculate_checksum();

        return (message, buff);
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
    fn new(
        socket: UdpSocket,
        rtt_estimate: Option<Duration>,
        keep_alive_interval: Option<Duration>,
    ) -> Self {
        let handle = Arc::new(Mutex::new(ConnectionHandle::new(
            socket,
            rtt_estimate,
            keep_alive_interval,
        )));
        let mut tasks = vec![];

        tasks.push(Self::resend_dropped_packets(Arc::clone(&handle)));
        tasks.push(Self::send_keep_alive_packets(Arc::clone(&handle)));
        tasks.push(Self::send_window_updates(Arc::clone(&handle)));

        Self { handle, tasks }
    }

    fn resend_dropped_packets(
        handle_arc: Arc<Mutex<ConnectionHandle>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if !handle_arc.lock().unwrap().is_active() {
                    break;
                }

                let rtt_estimate = { handle_arc.lock().unwrap().get_normalised_rtt_estimate() };

                tokio::time::delay_for(rtt_estimate).await;

                {
                    let mut handle = handle_arc.lock().unwrap();

                    if !handle.has_dropped_packet() {
                        continue;
                    }

                    handle.push_dropped_packet();
                }

                let flush_handle = Arc::clone(&handle_arc);
                futures::future::poll_fn(move |cx| flush_handle.lock().unwrap().poll_send_buff(cx))
                    .await
                    .unwrap();
            }
        })
    }

    fn send_keep_alive_packets(
        handle_arc: Arc<Mutex<ConnectionHandle>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if !handle_arc.lock().unwrap().is_active() {
                    break;
                }

                let keep_alive_interval = { handle_arc.lock().unwrap().keep_alive_interval };

                tokio::time::delay_for(keep_alive_interval).await;

                {
                    let mut handle = handle_arc.lock().unwrap();

                    let should_send_keep_alive = handle.last_send_at.is_none()
                        || (Instant::now() - handle.last_send_at.unwrap()) > keep_alive_interval;

                    if !should_send_keep_alive {
                        continue;
                    }

                    info!("sending keep alive");
                    handle.push_keep_alive_packet();
                }

                let flush_handle = Arc::clone(&handle_arc);
                futures::future::poll_fn(move |cx| flush_handle.lock().unwrap().poll_send_buff(cx))
                    .await
                    .unwrap();
            }
        })
    }

    fn send_window_updates(
        handle_arc: Arc<Mutex<ConnectionHandle>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if !handle_arc.lock().unwrap().is_active() {
                    break;
                }

                let original_window = { handle_arc.lock().unwrap().calculate_own_window() };
                let rtt_estimate = { handle_arc.lock().unwrap().get_normalised_rtt_estimate() };

                tokio::time::delay_for(rtt_estimate).await;

                {
                    let mut handle = handle_arc.lock().unwrap();

                    let current_window = { handle_arc.lock().unwrap().calculate_own_window() };

                    if original_window == 0 && current_window > 0 {
                        continue;
                    }

                    info!("sending window update");
                    handle.push_keep_alive_packet();
                }

                let flush_handle = Arc::clone(&handle_arc);
                futures::future::poll_fn(move |cx| flush_handle.lock().unwrap().poll_send_buff(cx))
                    .await
                    .unwrap();
            }
        })
    }
}

impl Drop for UdpConnection {
    fn drop(&mut self) {
        if self.handle.is_poisoned() {
            return;
        }

        let mut handle = self.handle.lock().unwrap();
        handle.close();
        debug!("Dropped udp connection handle");
    }
}

impl ConnectionHandle {
    fn new(
        socket: UdpSocket,
        rtt_estimate: Option<Duration>,
        keep_alive_interval: Option<Duration>,
    ) -> Self {
        let (recv, send) = socket.split();

        Self {
            state: State::Active,
            recv_sequence_number: 0,
            recv_message_buff: vec![],
            recv_buff: vec![],
            recv_state: Some(UdpRecvState::Idle(recv)),
            send_sequence_number: 0,
            send_buff: vec![],
            sending_packet: None,
            sent_packets: vec![],
            send_state: Some(UdpSendState::Idle(send)),
            last_send_at: None,
            send_zero_window_waker: None,
            peer_ack_number: 0,
            peer_window: MAX_RECV_BUFF,
            keep_alive_interval: keep_alive_interval
                .unwrap_or(Duration::from_millis(KEEP_ALIVE_INTERVAL as u64)),
            rtt_estimate: rtt_estimate
                .unwrap_or(Duration::from_millis(DEFAULT_RTT_ESTIMATE as u64)),
        }
    }

    fn is_active(&self) -> bool {
        self.state != State::Closed
    }

    fn close(&mut self) {
        // Signal all tasks to finish
        self.state = State::Closed;
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
            Ok(read) => {
                info!("received {} bytes", read);
                Ok(Vec::from(&buff[..read]))
            }
            Err(err) => Err(err),
        };

        (socket, result)
    }
}

impl ConnectionHandle {
    fn handle_recv(&mut self, mut data: Vec<u8>) -> IoResult<()> {
        while data.len() > 0 {
            let (result, remaining) = UdpMessage::parse(data);
            data = remaining;

            let result = match result {
                Ok(message) => self.handle_recv_message(message),
                Err(err) => {
                    error!("corrupted packet received: {:?}, {:?}", err, data);
                    return Ok(());
                }
            };

            match result {
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    error!("corrupted packet received: {:?}, {:?}", err, data)
                }
                Err(err) => return Err(err),
                Ok(_) => {}
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
        // maintaining ascending sequence number ordering
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
        self.handle_peer_ack_update();
        Ok(())
    }

    fn process_recv_messages(&mut self) {
        let mut consumed = 0;

        for message in self.recv_message_buff.iter() {
            if self.recv_sequence_number != message.sequence_number {
                info!(
                    "not next sequence number, actual {} != expected {}",
                    message.sequence_number, self.recv_sequence_number
                );
                break;
            }

            self.recv_buff.extend_from_slice(&message.payload[..]);
            self.recv_sequence_number = message.end_sequence_number();
            consumed += 1;
            info!("successfully received {} bytes", message.length);
        }

        self.recv_message_buff.drain(..consumed);
    }
}

impl AsyncRead for UdpConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<IoResult<usize>> {
        assert!(buff.len() > 0);

        let mut handle = self.handle.lock().unwrap();

        loop {
            let available_to_read = std::cmp::min(buff.len(), handle.recv_buff.len());

            if available_to_read > 0 {
                buff[..available_to_read].copy_from_slice(&handle.recv_buff[..available_to_read]);
                handle.recv_buff.drain(..available_to_read);

                // TODO: send window update

                return Poll::Ready(Ok(available_to_read));
            }

            let (new_state, result) = handle.recv_state.take().unwrap().poll(cx);
            handle.recv_state.replace(new_state);

            let data = match result {
                Poll::Ready(Ok(data)) => data,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            };

            match handle.handle_recv(data) {
                Ok(_) => {}
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}

impl UdpSendState {
    // This method copies the supplied buffer into it's future if it's in an idle state
    // this future is then polled on subsequent calls regardless of the supplied buffer
    // and it could return an IoResult::Ok with the amount of bytes sent from the
    // previous invocation. Hence it is only safe to use this method which does not
    // change the supplied buffer until it returns a Poll::Ready result.
    fn poll(self, cx: &mut Context<'_>, buff: &[u8]) -> (Self, Poll<IoResult<usize>>) {
        let mut future = match self {
            Self::Waiting(future) => future,
            Self::Idle(socket) => Self::await_for_next_send(socket, buff.to_vec()).boxed(),
        };

        match future.as_mut().poll(cx) {
            Poll::Pending => (Self::Waiting(future), Poll::Pending),
            Poll::Ready((socket, result)) => (Self::Idle(socket), Poll::Ready(result)),
        }
    }

    async fn await_for_next_send(
        mut socket: SendHalf,
        buff: Vec<u8>,
    ) -> (SendHalf, IoResult<usize>) {
        let result = tokio::select! {
            result = socket.send(&buff) => result,
            _ = tokio::time::delay_for(std::time::Duration::from_secs(SEND_TIMEOUT as u64)) => IoResult::Err(std::io::Error::from(std::io::ErrorKind::TimedOut))
        };

        (socket, result)
    }
}

impl ConnectionHandle {
    fn handle_peer_ack_update(&mut self) {
        if self.calculate_peer_window() > 0 {
            if let Some(waker) = self.send_zero_window_waker.take() {
                info!("window size increased, waking send operation");
                waker.wake();
            }
        }

        // ACK'd packets will not have to be resent
        // hence we can remove them from sent_packets
        let peer_ack_number = self.peer_ack_number;
        self.sent_packets
            .retain(|i| i.packet.ack_number > peer_ack_number);
    }

    fn poll_send_data(
        &mut self,
        cx: &mut Context<'_>,
        mut data: Vec<u8>,
    ) -> Poll<IoResult<Vec<u8>>> {
        if self.send_buff.len() == 0 {
            let (packet, remaining) = UdpMessage::create(
                self.send_sequence_number,
                self.recv_sequence_number,
                self.calculate_own_window(),
                self.calculate_peer_window(),
                data,
            );

            self.send_buff.extend_from_slice(&packet.to_vec()[..]);
            self.sending_packet.replace(packet);

            data = remaining;
        }

        let result = self.poll_send_buff(cx);

        match result {
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Ready(Ok(data)),
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(data)),
        }
    }

    fn poll_send_buff(&mut self, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        if self.send_buff.is_empty() {
            return Poll::Ready(Ok(()));
        }

        let (new_state, result) = self.send_state.take().unwrap().poll(cx, &self.send_buff);
        self.send_state.replace(new_state);

        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(sent)) => {
                info!("sent {} bytes", sent);
                self.send_buff.drain(..sent);

                // Append to sent_packets for resend if packets are dropped
                if self.send_buff.is_empty() {
                    let packet = self.sending_packet.take().unwrap();
                    self.send_sequence_number =
                        std::cmp::max(self.send_sequence_number, packet.end_sequence_number());
                    self.sent_packets.push(SentPacket {
                        packet,
                        sent_at: Instant::now(),
                    });
                    self.last_send_at.replace(Instant::now());
                }

                Poll::Ready(Ok(()))
            }
        }
    }

    fn has_dropped_packet(&mut self) -> bool {
        !self.sent_packets.is_empty() && self.has_dropped(self.sent_packets.first().unwrap())
    }

    fn pop_dropped_packet(&mut self) -> Option<UdpMessage> {
        match self.sent_packets.first() {
            Some(packet) if self.has_dropped(&packet) => Some(self.sent_packets.remove(0).packet),
            _ => None,
        }
    }

    fn has_dropped(&self, packet: &SentPacket) -> bool {
        return self.peer_ack_number < packet.packet.end_sequence_number()
            && Instant::now().duration_since(packet.sent_at)
                >= self.get_normalised_rtt_estimate() * 2;
    }

    fn push_dropped_packet(&mut self) {
        assert!(self.has_dropped_packet());
        assert!(self.send_buff.is_empty());

        let packet = self.pop_dropped_packet().unwrap();

        info!("resending dropped packet (seq: {})", packet.sequence_number);
        self.send_buff.extend_from_slice(&packet.to_vec()[..]);
        self.sending_packet.replace(packet);
    }

    fn push_keep_alive_packet(&mut self) {
        assert!(self.send_buff.is_empty());

        let (packet, _) = UdpMessage::create(
            self.send_sequence_number,
            self.recv_sequence_number,
            self.calculate_own_window(),
            self.calculate_peer_window(),
            vec![],
        );

        self.send_buff.extend_from_slice(&packet.to_vec()[..]);
        self.sending_packet.replace(packet);
    }

    fn get_normalised_rtt_estimate(&self) -> Duration {
        return std::cmp::max(Duration::from_millis(100), self.rtt_estimate);
    }

    fn calculate_own_window(&self) -> u32 {
        return std::cmp::max(
            0,
            MAX_RECV_BUFF
                - self.recv_buff.len() as u32
                - (self.recv_message_buff.len() * MAX_PACKET_SIZE) as u32,
        );
    }

    fn calculate_peer_window(&self) -> u32 {
        return std::cmp::max(
            0,
            self.peer_window - self.send_sequence_number - self.peer_ack_number,
        );
    }
}

impl AsyncWrite for UdpConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<IoResult<usize>> {
        assert!(buff.len() > 0);

        println!("buff: {:?}", buff);

        let mut handle = self.handle.lock().unwrap();

        // TODO: remove unnecessary copying
        let mut sending = Vec::from(buff);

        // TODO: Congestion control

        while sending.len() > 0 {
            if handle.calculate_peer_window() == 0 {
                info!("window size is 0, waiting for window update");
                handle.send_zero_window_waker.replace(cx.waker().clone());
                return Poll::Pending;
            }

            sending = match handle.poll_send_data(cx, sending) {
                Poll::Pending => {
                    println!("pending");
                    return Poll::Pending;
                }
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Ready(Ok(remaining)) => remaining,
            };
        }

        println!("ok (written: {})", buff.len() - sending.len());
        Poll::Ready(Ok(buff.len() - sending.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        info!("flushing udp connection");
        let mut handle = self.handle.lock().unwrap();
        handle.poll_send_buff(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        todo!()
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
                Ok(rtt_estimate) => Ok(UdpConnection::new(socket, Some(rtt_estimate), None)),
                Err(err) => Err(err)
            },
            _ = tokio::time::delay_for(Duration::from_secs(3)) => {
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
) -> Result<Duration> {
    let peer_addr =
        peer_info.peer_ip_address.clone() + ":" + &connection_info.peer_listen_port.to_string();
    let initiated_at = Instant::now();

    info!("Attempting to connect to {}", peer_addr);
    send_hello(&mut socket, Some(peer_addr)).await?;
    let peer_addr = wait_for_hello(&mut socket).await?;

    info!("Connected to {}", peer_addr);
    socket.connect(peer_addr).await?;

    send_hello(&mut socket, Option::None::<String>).await?;

    let rtt_estimate = Instant::now() - initiated_at;
    info!(
        "Connection confirmed with {} (rtt estimate: {:?})",
        peer_addr, rtt_estimate
    );

    Ok(rtt_estimate)
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
    use lazy_static::lazy_static;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_parse_udp_message() {
        let raw_data = vec![
            0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1, 2, 3, 4, 5, 6,
        ];

        let (message, remaining) = UdpMessage::parse(raw_data);
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

        let (message, remaining) = UdpMessage::parse(raw_data);

        assert!(message.is_err());
        assert_eq!(remaining, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_parse_udp_message_not_enough_payload() {
        let raw_data = vec![0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1];

        let (message, _) = UdpMessage::parse(raw_data);

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
    fn test_udp_message_create() {
        let (message, remaining) = UdpMessage::create(100, 200, 300, 10, vec![1, 2, 3, 4, 5]);

        assert_eq!(message.sequence_number, 100);
        assert_eq!(message.ack_number, 200);
        assert_eq!(message.window, 300);
        assert_eq!(message.length, 5);
        assert!(message.is_checksum_valid());
        assert_eq!(message.payload, vec![1, 2, 3, 4, 5]);
        assert_eq!(remaining, Vec::<u8>::new());
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

        let (parsed_message, remaining) = UdpMessage::parse(message.to_vec());
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

    lazy_static! {
        static ref UDP_PORT_NUMBER: Mutex<u16> = Mutex::from(25660);
    }

    async fn init_new_udp_connection_raw_half() -> (UdpConnection, UdpSocket) {
        let (port1, port2) = {
            let mut port = UDP_PORT_NUMBER.lock().unwrap();

            *port += 2;
            (*port, *port - 1)
        };

        let socket1 = UdpSocket::bind("0.0.0.0:".to_owned() + &port1.to_string())
            .await
            .unwrap();

        socket1
            .connect("127.0.0.1:".to_owned() + &port2.to_string())
            .await
            .unwrap();

        let socket2 = UdpSocket::bind("0.0.0.0:".to_owned() + &port2.to_string())
            .await
            .unwrap();
        socket2
            .connect("127.0.0.1:".to_owned() + &port1.to_string())
            .await
            .unwrap();

        let connection = UdpConnection::new(socket1, None, Some(Duration::from_secs(1)));

        return (connection, socket2);
    }

    #[test]
    fn test_connection_recv_valid_message() {
        Runtime::new().unwrap().block_on(async {
            let (mut connection, mut raw_socket) = init_new_udp_connection_raw_half().await;

            let mut message = UdpMessage {
                sequence_number: 0,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![1, 2, 3],
            };

            message.checksum = message.calculate_checksum();

            raw_socket.send(message.to_vec().as_slice()).await.unwrap();

            let mut buff = [0u8; 1024];
            let read = connection.read(&mut buff).await.unwrap();

            assert_eq!(buff[..read], [1, 2, 3]);

            let handle = connection.handle.lock().unwrap();
            assert_eq!(handle.recv_buff, Vec::<u8>::new());
            assert_eq!(handle.recv_message_buff, Vec::<UdpMessage>::new());
            assert_eq!(handle.recv_sequence_number, 3);
            assert_eq!(handle.peer_ack_number, 1);
            assert_eq!(handle.peer_window, 2);
        });
    }

    #[test]
    fn test_connection_recv_out_of_order_messages() {
        Runtime::new().unwrap().block_on(async {
            let (mut connection, mut raw_socket) = init_new_udp_connection_raw_half().await;

            let mut message1 = UdpMessage {
                sequence_number: 3,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![4, 5, 6],
            };
            message1.checksum = message1.calculate_checksum();

            raw_socket.send(message1.to_vec().as_slice()).await.unwrap();

            let mut message2 = UdpMessage {
                sequence_number: 0,
                ack_number: 1,
                window: 2,
                length: 3,
                checksum: 0,
                payload: vec![1, 2, 3],
            };
            message2.checksum = message2.calculate_checksum();

            raw_socket.send(message2.to_vec().as_slice()).await.unwrap();

            let mut buff = [0u8; 1024];
            let read = connection.read(&mut buff).await.unwrap();

            assert_eq!(buff[..read], [1, 2, 3, 4, 5, 6]);

            let mut handle = connection.handle.lock().unwrap();
            assert_eq!(handle.recv_buff, Vec::<u8>::new());
            assert_eq!(handle.recv_message_buff, Vec::<UdpMessage>::new());
            assert_eq!(handle.recv_sequence_number, 6);
            assert_eq!(handle.peer_ack_number, 1);
            assert_eq!(handle.peer_window, 2);
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

            for i in 1..10 {
                connection1
                    .write((i.to_string() + " -- hello from 1").as_bytes())
                    .await
                    .unwrap();

                let mut buff = [0; 1024];
                let read = connection2.read(&mut buff).await.unwrap();

                assert_eq!(
                    String::from_utf8(buff[..read].to_vec()).unwrap(),
                    (i.to_string() + " -- hello from 1")
                );

                connection2
                    .write((i.to_string() + " -- hello from 2").as_bytes())
                    .await
                    .unwrap();

                let mut buff = [0; 1024];
                let read = connection1.read(&mut buff).await.unwrap();

                assert_eq!(
                    String::from_utf8(buff[..read].to_vec()).unwrap(),
                    (i.to_string() + " -- hello from 2")
                );
            }
        });
    }

    #[test]
    fn test_connection_resends_dropped_packet() {
        env_logger::init();
        Runtime::new().unwrap().block_on(async {
            let (mut connection, mut raw_socket) = init_new_udp_connection_raw_half().await;

            {
                let mut handle = connection.handle.lock().unwrap();
                handle.rtt_estimate = Duration::from_millis(1000);
            }

            let buff = [1, 2, 3, 4];
            connection.write_all(&buff).await.unwrap();
            connection.flush().await.unwrap();

            {
                let handle = connection.handle.lock().unwrap();

                assert_eq!(
                    handle
                        .sent_packets
                        .iter()
                        .map(|i| &i.packet)
                        .collect::<Vec<&UdpMessage>>(),
                    vec![
                        &UdpMessage::create(
                            0,
                            0,
                            MAX_RECV_BUFF,
                            MAX_RECV_BUFF,
                            [1, 2, 3, 4].to_vec()
                        )
                        .0,
                    ]
                );
            }

            // Wait for resend timeout
            {
                let handle = connection.handle.lock().unwrap();
                tokio::time::delay_for(handle.get_normalised_rtt_estimate() * 3).await;
            }

            // Trigger another write to force resend
            let buff = [5, 6, 7, 8];
            connection.write_all(&buff).await.unwrap();

            let mut messages = vec![];

            for _ in 1..=3 {
                let mut buff = [0u8; 1024];

                let read = tokio::select! {
                    result = raw_socket.recv(&mut buff[..]) => result.unwrap(),
                    _ = tokio::time::delay_for(Duration::from_millis(100)) => break,
                };

                messages.push(UdpMessage::parse(buff[..read].to_vec()).0.unwrap());
            }

            assert_eq!(
                messages,
                vec![
                    UdpMessage::create(0, 0, MAX_RECV_BUFF, MAX_RECV_BUFF, [1, 2, 3, 4].to_vec()).0,
                    UdpMessage::create(0, 0, MAX_RECV_BUFF, MAX_RECV_BUFF, [1, 2, 3, 4].to_vec()).0,
                    UdpMessage::create(4, 0, MAX_RECV_BUFF, MAX_RECV_BUFF, [5, 6, 7, 8].to_vec()).0,
                ]
            );

            // Send ACK for received packets.
            raw_socket
                .send(
                    UdpMessage::create(0, 8, MAX_RECV_BUFF, MAX_RECV_BUFF, vec![1])
                        .0
                        .to_vec()
                        .as_slice(),
                )
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = connection.read(&mut buff).await.unwrap();

            assert_eq!(&buff[..read], &[1]);

            // Since all packets have been ACK'd the sent packet vec
            // should have been cleared
            {
                let handle = connection.handle.lock().unwrap();
                assert_eq!(handle.sent_packets, vec![]);
            }
        });
    }

    #[test]
    fn test_connection_sends_keep_alive_packets() {
        Runtime::new().unwrap().block_on(async {
            let (connection, mut raw_socket) = init_new_udp_connection_raw_half().await;

            // Wait for keep alive timeout
            let keep_alive_interval = { connection.handle.lock().unwrap().keep_alive_interval };

            tokio::time::delay_for(keep_alive_interval).await;

            let mut messages = vec![];

            for _ in 1..=2 {
                let mut buff = [0u8; 1024];

                let read = tokio::select! {
                    result = raw_socket.recv(&mut buff[..]) => result.unwrap(),
                    _ = tokio::time::delay_for(Duration::from_millis(100)) => break,
                };

                messages.push(UdpMessage::parse(buff[..read].to_vec()).0.unwrap());
            }

            assert_eq!(
                messages,
                vec![UdpMessage::create(0, 0, MAX_RECV_BUFF, MAX_RECV_BUFF, [].to_vec()).0,]
            );
        });
    }
}
