use super::{
    schedule_resend_if_dropped, SendEvent, SendEventReceiver, SequenceNumber, UdpConnectionVars,
    UdpPacket,
};
use anyhow::{Error, Result};
use log::*;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::delay_for;

pub(super) struct UdpConnectionOrchestrator {
    socket: Option<UdpSocket>,
    con: Arc<Mutex<UdpConnectionVars>>,
    send_receiver: SendEventReceiver,
}

impl UdpConnectionOrchestrator {
    pub(super) fn new(
        socket: UdpSocket,
        con: Arc<Mutex<UdpConnectionVars>>,
        send_receiver: UnboundedReceiver<SendEvent>,
    ) -> Self {
        Self {
            socket: Some(socket),
            con,
            send_receiver: SendEventReceiver(send_receiver),
        }
    }

    pub(super) fn start_orchestration_loop(mut self) {
        tokio::spawn(async move {
            let (mut socket_recv, mut socket_send) = self.socket.take().unwrap().split();

            let (keep_alive_interval, recv_timeout) = {
                let con = self.con.lock().unwrap();
                let config = con.config();

                (config.keep_alive_interval(), config.recv_timeout())
            };

            let mut current_recv_timeout = recv_timeout;
            let mut current_keep_alive_timeout = keep_alive_interval;
            let mut recv_buff = [0u8; 1024];

            loop {
                {
                    let con = self.con.lock().unwrap();

                    if !con.is_connected() {
                        info!("connection disconnected, stopping orchestration loop");
                        break;
                    }
                }

                let before = Instant::now();

                let result = tokio::select! {
                    //
                    result = socket_recv.recv(&mut recv_buff) => match result {
                        Ok(read) => {
                            current_recv_timeout = recv_timeout;
                            handle_recv_packet(Arc::clone(&self.con), &recv_buff[..read])
                        },
                        Err(err) => Err(Error::from(err))
                    },
                    _ = delay_for(current_recv_timeout) => handle_recv_timeout(Arc::clone(&self.con)),
                    //
                    event = self.send_receiver.wait_for_next_packet(Arc::clone(&self.con)) => match event {
                        Some(packet) => {
                            current_keep_alive_timeout = keep_alive_interval;
                            handle_send_packet(Arc::clone(&self.con), packet, &mut socket_send).await
                        },
                        None => {
                            info!("send channel has been dropped");
                            break;
                        }
                    },
                    _ = delay_for(current_keep_alive_timeout) => handle_keep_alive(Arc::clone(&self.con), &mut socket_send)
                };

                if let Err(err) = result {
                    error!("error during orchestration loop: {}", err);
                    break;
                }

                let duration = Instant::now().duration_since(before);
                current_recv_timeout -= duration;
                current_keep_alive_timeout -= duration;
            }
        });
    }
}

fn handle_recv_packet(con: Arc<Mutex<UdpConnectionVars>>, packet: &[u8]) -> Result<()> {
    let packet = match UdpPacket::parse(packet) {
        Ok(packet) => packet,
        Err(error) => {
            error!("could not parse packet from incoming datagram");
            return Ok(());
        }
    };

    if !packet.is_checksum_valid() {
        error!(
            "received packet with invalid sequence number ({}), discarding",
            packet.sequence_number
        );
        return Ok(());
    }

    let mut con = con.lock().unwrap();

    con.update_peer_ack_number(packet.sequence_number);
    con.adjust_rtt_estimate(&packet);

    if let Err(err) = con.recv_process_packet(packet) {
        error!("error while receiving packet: {}", err);
    }

    Ok(())
}

fn handle_recv_timeout(con: Arc<Mutex<UdpConnectionVars>>) -> Result<()> {
    todo!()
}

async fn handle_send_packet(
    con: Arc<Mutex<UdpConnectionVars>>,
    packet: UdpPacket,
    socket_send: &mut SendHalf,
) -> Result<()> {
    match socket_send.send(&packet.to_vec()[..]).await {
        Ok(_) => {}
        Err(err) => return Err(Error::from(err)),
    }

    {
        let mut con = con.lock().unwrap();

        con.store_send_time_of_packet(&packet);
        con.increase_transit_window_after_send();
    }

    schedule_resend_if_dropped(con, packet);

    Ok(())
}

fn handle_keep_alive(con: Arc<Mutex<UdpConnectionVars>>, socket_send: &mut SendHalf) -> Result<()> {
    todo!()
}
