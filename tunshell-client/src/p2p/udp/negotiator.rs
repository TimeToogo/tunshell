use super::{SequenceNumber, UdpConnectionState, UdpConnectionVars, UdpPacket, UdpPacketType};
use anyhow::{Context, Error, Result};
use log::*;
use rand::Rng;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::delay_for;

/// This magic string is exchanged between peers
/// in verifying that they are both attempting to establish a connection
/// with each other
const MAGIC_HELLO: &[u8] = "tunshell::udp::hello".as_bytes();

/// Attempts to negotiate a connection over UDP between two peers.
/// This should support a connection where at most one peer is behind a NAT.
async fn negotiate_connection(
    con: &mut UdpConnectionVars,
    socket: &mut UdpSocket,
    peer_ip: IpAddr,
    suggested_port: u16,
    master_side: bool,
) -> Result<SocketAddr> {
    assert!(con.state == UdpConnectionState::New);

    let timeout = con.config().connect_timeout();

    // Send the first hello packet.
    // This may not be delivered if the peer is behind a NAT
    send_magic_hello(socket, Some(SocketAddr::from((peer_ip, suggested_port)))).await?;
    con.set_state_sent_hello();

    // Wait for a hello packet.
    // If one of the peers is not behind a NAT, this received the hello packet.
    // We then connect on the outbound port received from the hello packet.
    let peer_addr = wait_for_magic_hello(socket, peer_ip, timeout).await?;
    socket
        .connect(peer_addr)
        .await
        .map_err(|err| Error::from(err))?;

    // We send a second hello packet to the same port as received from the hello packet.
    // In the case where neither peers are behind NAT's the packet can be ignored.
    send_magic_hello(socket, None).await?;

    let start = Instant::now();

    // If we are the master side we first send the sync packet and wait for the reply
    // otherwise we wait for the sync and then send a reply
    if master_side {
        send_sync_packet(socket, con).await?;
        con.set_state_sent_sync();

        wait_for_sync_packet(socket, con, timeout).await?;

        // Since we have completed a full round trip we set the initial RTT estimate
        con.rtt_estimate = Instant::now().duration_since(start);
    } else {
        con.set_state_waiting_for_sync();

        wait_for_sync_packet(socket, con, timeout).await?;
        send_sync_packet(socket, con).await?;

        // Considering the non-master side only has to wait for the sync packet to arrive
        // we make the naive assumption the RTT is symmetrical to start.
        con.rtt_estimate = Instant::now().duration_since(start) * 2;
    }

    Ok(peer_addr)
}

// Send the magic hello sequence to the peer IP on the suggested port
async fn send_magic_hello(socket: &mut UdpSocket, dest: Option<SocketAddr>) -> Result<()> {
    let send = match dest {
        Some(dest) => socket.send_to(MAGIC_HELLO, dest).await,
        None => socket.send(MAGIC_HELLO).await,
    };

    match send {
        Ok(_) => Ok(()),
        Err(err) => Err(Error::from(err)).context("failed to send magic hello packet"),
    }
}

// Waits for the magic hello from the peer.
// This could come from potentially a different port on the peer
// than provided as, due to potential NAT's, we do not know the
// outbound port ahead of time.
async fn wait_for_magic_hello(
    socket: &mut UdpSocket,
    peer_ip: IpAddr,
    timeout: Duration,
) -> Result<SocketAddr> {
    let mut buff = [0u8; MAGIC_HELLO.len()];

    let (read, from_addr) = tokio::select! {
        result = socket.recv_from(&mut buff) => match result {
            Ok((read, addr)) => (read, addr),
            Err(err) => return Err(Error::from(err)).context("failed to wait for magic hello packet"),
        },
        _ = delay_for(timeout) => return Err(Error::msg("timed out while waiting for magic hello"))
    };
    if &buff[..read] != MAGIC_HELLO {
        return Err(Error::msg(
            "received packet containing data other than magic hello",
        ));
    }

    if from_addr.ip() != peer_ip {
        return Err(Error::msg(format!(
            "expected packet to be received from {} but received from {}",
            peer_ip,
            from_addr.ip()
        )));
    }

    Ok(from_addr)
}

async fn send_sync_packet(socket: &mut UdpSocket, con: &mut UdpConnectionVars) -> Result<()> {
    // We need to keep the rng in a block so rust knows it is dropped before an await yield
    // So this async function can be thread-safe (Send)
    {
        // We initialise the connection state to a random sequence number
        let mut rng = rand::thread_rng();
        con.sequence_number = SequenceNumber(rng.gen());
        debug!("initialised sequence number to {}", con.sequence_number);
    }

    match socket
        .send(con.create_open_packet().to_vec().as_slice())
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Error::from(err)).context("failed to send magic hello packet"),
    }
}

async fn wait_for_sync_packet(
    socket: &mut UdpSocket,
    con: &mut UdpConnectionVars,
    timeout: Duration,
) -> Result<()> {
    let mut buff = [0u8; 256];

    let packet_buff = loop {
        let read = tokio::select! {
            result = socket.recv(&mut buff) => match result {
                Ok(read) => read,
                Err(err) => return Err(Error::from(err)).context("failed to wait for sync packet"),
            },
            _ = delay_for(timeout) => return Err(Error::msg("timed out while waiting for sync"))
        };

        // If neither of the peers are behind NAT's
        // The non-master peer may receive a second magic hello packet.
        // We ignore this here
        if read == MAGIC_HELLO.len() && &buff[..read] == MAGIC_HELLO {
            continue;
        }

        // I'm not certain this is possible but if the magic packet
        // is combined into the same buffer as the sync packet we
        // ignore this here
        if read > MAGIC_HELLO.len() && &buff[..read] == MAGIC_HELLO {
            break &buff[MAGIC_HELLO.len()..(read - MAGIC_HELLO.len())];
        }

        break &buff[..read];
    };

    let sync_packet = UdpPacket::parse(packet_buff)?;

    match sync_packet.packet_type {
        UdpPacketType::Open => {}
        _ => return Err(Error::msg("invalid sync packet type")),
    }

    con.peer_window = sync_packet.window;
    con.ack_number = sync_packet.sequence_number;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::UdpConnectionConfig;
    use super::*;
    use lazy_static::lazy_static;
    use std::sync::Mutex;
    use tokio::runtime::Runtime;

    lazy_static! {
        static ref UDP_PORT_NUMBER: Mutex<u16> = Mutex::from(26660);
    }

    async fn init_udp_socket_pair() -> (UdpSocket, UdpSocket) {
        let (port1, port2) = {
            let mut port = UDP_PORT_NUMBER.lock().unwrap();

            *port += 2;
            (*port, *port - 1)
        };

        let socket1 = UdpSocket::bind("127.0.0.1:".to_owned() + &port1.to_string())
            .await
            .unwrap();

        let socket2 = UdpSocket::bind("127.0.0.1:".to_owned() + &port2.to_string())
            .await
            .unwrap();

        return (socket1, socket2);
    }

    #[test]
    fn test_negotiate_connection() {
        Runtime::new().unwrap().block_on(async {
            let (mut socket1, mut socket2) = init_udp_socket_pair().await;

            let addr1 = socket1.local_addr().unwrap();
            let addr2 = socket2.local_addr().unwrap();

            let config =
                UdpConnectionConfig::default().with_connect_timeout(Duration::from_millis(50));
            let mut con1 = UdpConnectionVars::new(config.clone());
            let mut con2 = UdpConnectionVars::new(config.clone());

            let (result1, result2) = tokio::join!(
                negotiate_connection(&mut con1, &mut socket1, addr2.ip(), addr2.port(), true),
                negotiate_connection(&mut con2, &mut socket2, addr1.ip(), addr1.port(), false)
            );

            assert_eq!(result1.unwrap(), addr2);
            assert_eq!(result2.unwrap(), addr1);
        });
    }

    #[test]
    fn test_negotiate_connection_with_one_incorrect_port() {
        Runtime::new().unwrap().block_on(async {
            let (mut socket1, mut socket2) = init_udp_socket_pair().await;

            let addr1 = socket1.local_addr().unwrap();
            let addr2 = socket2.local_addr().unwrap();

            let config =
                UdpConnectionConfig::default().with_connect_timeout(Duration::from_millis(50));
            let mut con1 = UdpConnectionVars::new(config.clone());
            let mut con2 = UdpConnectionVars::new(config.clone());

            // In the event of an NAT on one side of the connection
            // the suggested port will not be correct
            // The connection should still be able to be made by listening for the
            // outbound port of the incoming hello packet.
            let (result1, result2) = tokio::join!(
                negotiate_connection(&mut con1, &mut socket1, addr2.ip(), 20000, true),
                negotiate_connection(&mut con2, &mut socket2, addr1.ip(), addr1.port(), false)
            );

            assert_eq!(result1.unwrap(), addr2);
            assert_eq!(result2.unwrap(), addr1);
        });
    }

    #[test]
    fn test_negotiate_connection_with_two_incorrect_ports() {
        Runtime::new().unwrap().block_on(async {
            let (mut socket1, mut socket2) = init_udp_socket_pair().await;

            let addr1 = socket1.local_addr().unwrap();
            let addr2 = socket2.local_addr().unwrap();

            let config =
                UdpConnectionConfig::default().with_connect_timeout(Duration::from_millis(50));
            let mut con1 = UdpConnectionVars::new(config.clone());
            let mut con2 = UdpConnectionVars::new(config.clone());

            // If both sides of the connections are behind port-mangling NAT's
            // the connection will not succeeded.
            let (result1, result2) = tokio::join!(
                negotiate_connection(&mut con1, &mut socket1, addr2.ip(), 1, true),
                negotiate_connection(&mut con2, &mut socket2, addr1.ip(), 2, false)
            );

            assert_eq!(result1.is_err(), true);
            assert_eq!(result2.is_err(), true);
        });
    }

    #[test]
    fn test_negotiate_connection_master_side() {
        Runtime::new().unwrap().block_on(async {
            let (mut socket1, mut socket2) = init_udp_socket_pair().await;

            let addr1 = socket1.local_addr().unwrap();
            let addr2 = socket2.local_addr().unwrap();

            let config = UdpConnectionConfig::default()
                .with_recv_window(1000)
                .with_connect_timeout(Duration::from_millis(500));
            let mut con1 = UdpConnectionVars::new(config.clone());

            let task = tokio::spawn(async move {
                negotiate_connection(&mut con1, &mut socket1, addr2.ip(), addr2.port(), true)
                    .await
                    .unwrap();

                con1
            });

            socket2.connect(addr1).await.unwrap();

            let mut buff = [0u8; 1024];

            // Should receive hello
            let read = socket2.recv(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], MAGIC_HELLO);

            // Mock response
            socket2.send(MAGIC_HELLO).await.unwrap();

            // Should receive second hello
            let read = socket2.recv(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], MAGIC_HELLO);

            // Should receive sync packet from master
            let read = socket2.recv(&mut buff).await.unwrap();
            let sync_packet = UdpPacket::parse(&buff[..read]).unwrap();
            assert_eq!(sync_packet.packet_type, UdpPacketType::Open);
            assert_eq!(sync_packet.window, 1000);

            // Mock send reply packet
            let reply_packet = UdpPacket::open(SequenceNumber(1), SequenceNumber(0), 5000);
            socket2
                .send(reply_packet.to_vec().as_slice())
                .await
                .unwrap();

            // Connection should be complete
            let con1 = task.await.unwrap();

            assert_eq!(sync_packet.sequence_number, con1.sequence_number);
            assert_eq!(con1.peer_window, 5000);
            assert_eq!(con1.state, UdpConnectionState::SentSync);
        });
    }

    #[test]
    fn test_negotiate_connection_non_master_side() {
        Runtime::new().unwrap().block_on(async {
            let (mut socket1, mut socket2) = init_udp_socket_pair().await;

            let addr1 = socket1.local_addr().unwrap();
            let addr2 = socket2.local_addr().unwrap();

            let config = UdpConnectionConfig::default()
                .with_recv_window(1000)
                .with_connect_timeout(Duration::from_millis(500));
            let mut con1 = UdpConnectionVars::new(config.clone());

            let task = tokio::spawn(async move {
                negotiate_connection(&mut con1, &mut socket1, addr2.ip(), addr2.port(), false)
                    .await
                    .unwrap();

                con1
            });

            socket2.connect(addr1).await.unwrap();

            let mut buff = [0u8; 1024];

            // Should receive hello
            let read = socket2.recv(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], MAGIC_HELLO);

            // Mock response
            socket2.send(MAGIC_HELLO).await.unwrap();

            // Should receive second hello
            let read = socket2.recv(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], MAGIC_HELLO);

            // Mock send sync packet
            let reply_packet = UdpPacket::open(SequenceNumber(100), SequenceNumber(0), 5000);
            socket2
                .send(reply_packet.to_vec().as_slice())
                .await
                .unwrap();

            // Should receive reply packet from master
            let read = socket2.recv(&mut buff).await.unwrap();
            let reply_packet = UdpPacket::parse(&buff[..read]).unwrap();
            assert_eq!(reply_packet.packet_type, UdpPacketType::Open);
            assert_eq!(reply_packet.window, 1000);
            assert_eq!(reply_packet.ack_number, SequenceNumber(100));

            // Connection should be complete
            let con1 = task.await.unwrap();

            assert_eq!(reply_packet.sequence_number, con1.sequence_number);
            assert_eq!(con1.peer_window, 5000);
            assert_eq!(con1.state, UdpConnectionState::WaitingForSync);
        });
    }
}
