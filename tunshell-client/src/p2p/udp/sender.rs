use super::{SequenceNumber, UdpConnectionVars, UdpPacket};
use log::*;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, PartialEq)]
pub(super) enum SendEvent {
    Send(UdpPacket),
    Resend(UdpPacket),
    AckUpdate,
    WindowUpdate,
}

pub(super) struct SendEventReceiver(pub(super) UnboundedReceiver<SendEvent>);

impl SendEventReceiver {
    pub(super) async fn wait_for_next_packet(
        &mut self,
        con: Arc<Mutex<UdpConnectionVars>>,
    ) -> Option<UdpPacket> {
        let mut event = match self.0.recv().await {
            Some(event) => event,
            None => return None,
        };

        for _ in 1..5 {
            match event {
                SendEvent::Send(packet) | SendEvent::Resend(packet) => return Some(packet),
                // In the case of ack or window update we yield to executor multiple times
                // to avoid sending unnecessary empty packets if they are not required
                SendEvent::AckUpdate | SendEvent::WindowUpdate => tokio::task::yield_now().await,
            };

            event = match self.0.try_recv() {
                Ok(event) => event,
                Err(_) => continue,
            };
        }

        // After yielding multiple times, if there are still no pending packets
        // to be sent, we send an empty packet
        let mut con = con.lock().unwrap();

        Some(con.create_next_packet(&[]))
    }
}

impl UdpConnectionVars {
    pub(super) fn create_next_packet(&mut self, payload: &[u8]) -> UdpPacket {
        let packet = UdpPacket::create(
            self.sequence_number + SequenceNumber(1),
            self.ack_number,
            self.calculate_recv_window(),
            payload,
        );

        self.sequence_number = packet.end_sequence_number();

        packet
    }
}

#[cfg(test)]
mod tests {
    use super::super::UdpConnectionConfig;
    use super::*;
    use tokio::runtime::Runtime;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn test_create_new_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));

        con.sequence_number = SequenceNumber(10);
        con.ack_number = SequenceNumber(20);

        let packet = con.create_next_packet(&[]);

        assert_eq!(packet.sequence_number, SequenceNumber(11));
        assert_eq!(packet.ack_number, SequenceNumber(20));
        assert_eq!(packet.window, 1000);
        assert_eq!(packet.payload, Vec::<u8>::new());

        assert_eq!(con.sequence_number, SequenceNumber(11));
    }

    #[test]
    fn test_wait_for_next_packet_send_event() {
        Runtime::new().unwrap().block_on(async {
            let con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            let sent_packet = UdpPacket::create(SequenceNumber(1), SequenceNumber(1), 0, &[]);

            tx.send(SendEvent::Send(sent_packet.clone())).unwrap();

            let packet = task.await.unwrap().unwrap();

            assert_eq!(packet, sent_packet);
        });
    }

    #[test]
    fn test_wait_for_next_packet_resend_event() {
        Runtime::new().unwrap().block_on(async {
            let con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            let sent_packet = UdpPacket::create(SequenceNumber(1), SequenceNumber(1), 0, &[]);

            tx.send(SendEvent::Resend(sent_packet.clone())).unwrap();

            let packet = task.await.unwrap().unwrap();

            assert_eq!(packet, sent_packet);
        });
    }

    #[test]
    fn test_wait_for_next_packet_dropped_sender() {
        Runtime::new().unwrap().block_on(async {
            let con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();
            drop(tx);

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            let option = task.await.unwrap();

            assert_eq!(option, None);
        });
    }

    #[test]
    fn test_wait_for_next_packet_ack_update_event() {
        Runtime::new().unwrap().block_on(async {
            let mut con =
                UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));
            con.sequence_number = SequenceNumber(10);

            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            tx.send(SendEvent::AckUpdate).unwrap();

            let packet = task.await.unwrap().unwrap();

            assert_eq!(
                packet,
                UdpPacket::create(SequenceNumber(11), SequenceNumber(0), 1000, &[])
            );
        });
    }

    #[test]
    fn test_wait_for_next_packet_window_update_event() {
        Runtime::new().unwrap().block_on(async {
            let mut con =
                UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));
            con.sequence_number = SequenceNumber(10);

            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            tx.send(SendEvent::WindowUpdate).unwrap();

            let packet = task.await.unwrap().unwrap();

            assert_eq!(
                packet,
                UdpPacket::create(SequenceNumber(11), SequenceNumber(0), 1000, &[])
            );
        });
    }

    #[test]
    fn test_wait_for_next_packet_ack_window_updates_followed_by_send() {
        Runtime::new().unwrap().block_on(async {
            let con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let con = Arc::new(Mutex::new(con));

            let (tx, rx) = unbounded_channel();

            let mut receiver = SendEventReceiver(rx);

            let task =
                tokio::spawn(async move { receiver.wait_for_next_packet(Arc::clone(&con)).await });

            let sent_packet = UdpPacket::create(SequenceNumber(1), SequenceNumber(1), 0, &[]);

            tx.send(SendEvent::AckUpdate).unwrap();
            tx.send(SendEvent::WindowUpdate).unwrap();
            tx.send(SendEvent::Send(sent_packet.clone())).unwrap();

            let packet = task.await.unwrap().unwrap();

            assert_eq!(packet, sent_packet);
        });
    }
}
