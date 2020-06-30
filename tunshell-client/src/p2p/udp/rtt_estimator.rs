use super::{UdpConnectionVars, UdpPacket};
use std::time::{Duration, Instant};

impl UdpConnectionVars {
    pub(super) fn store_send_time_of_packet(&mut self, sent_packet: &UdpPacket) {
        let corresponding_ack_number = sent_packet.end_sequence_number();
        self.send_times
            .insert(corresponding_ack_number, Instant::now());
    }

    pub(super) fn adjust_rtt_estimate(&mut self, recv_packet: &UdpPacket) {
        let sent_at = match self.send_times.get(&recv_packet.ack_number) {
            Some(sent_at) => sent_at,
            None => return,
        };

        let rtt_measurement = Instant::now().duration_since(*sent_at);

        // The rtt estimate is kept by using a moving average.
        // This is calculated using the average between the previous value and the new measurement.
        if self.rtt_estimate.as_nanos() == 0 {
            self.rtt_estimate = rtt_measurement;
        } else {
            self.rtt_estimate = Duration::from_nanos(
                ((self.rtt_estimate.as_nanos() + rtt_measurement.as_nanos()) / 2) as u64,
            );
        }

        // We also clear any sent times from sent packets which may have
        // not been acknowledged individually.
        self.send_times.retain(|k, _| *k > recv_packet.ack_number)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SequenceNumber, UdpConnectionConfig};
    use super::*;

    #[test]
    fn test_store_send_time_of_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.store_send_time_of_packet(&UdpPacket::data(
            SequenceNumber(50),
            SequenceNumber(0),
            0,
            &[1],
        ));

        assert_eq!(con.send_times.get(&SequenceNumber(51)).is_some(), true);
    }

    #[test]
    fn test_adjust_rtt_estimate_with_packet_not_in_sent_time() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.rtt_estimate = Duration::from_millis(100);

        con.adjust_rtt_estimate(&UdpPacket::data(
            SequenceNumber(50),
            SequenceNumber(10),
            0,
            &[],
        ));

        assert_eq!(con.rtt_estimate, Duration::from_millis(100));
    }

    #[test]
    fn test_adjust_rtt_estimate_with_sent_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.rtt_estimate = Duration::from_millis(0);

        con.send_times.insert(
            SequenceNumber(100),
            Instant::now() - Duration::from_millis(100),
        );

        con.adjust_rtt_estimate(&UdpPacket::data(
            SequenceNumber(50),
            SequenceNumber(100),
            0,
            &[],
        ));

        assert_eq!((con.rtt_estimate.as_millis() as i32 - 100).abs() < 5, true);
    }

    #[test]
    fn test_adjust_rtt_estimate_with_sent_packet_average() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.rtt_estimate = Duration::from_millis(150);

        con.send_times.insert(
            SequenceNumber(100),
            Instant::now() - Duration::from_millis(100),
        );

        con.adjust_rtt_estimate(&UdpPacket::data(
            SequenceNumber(50),
            SequenceNumber(100),
            0,
            &[],
        ));

        assert_eq!((con.rtt_estimate.as_millis() as i32 - 125).abs() < 5, true);
    }

    #[test]
    fn test_adjust_rtt_estimate_removes_acknowledged_sent_times() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.rtt_estimate = Duration::from_millis(150);

        con.send_times.insert(SequenceNumber(50), Instant::now());
        con.send_times.insert(SequenceNumber(100), Instant::now());
        con.send_times.insert(SequenceNumber(150), Instant::now());

        con.adjust_rtt_estimate(&UdpPacket::data(
            SequenceNumber(50),
            SequenceNumber(100),
            0,
            &[],
        ));

        assert_eq!(
            con.send_times
                .keys()
                .copied()
                .collect::<Vec<SequenceNumber>>(),
            vec![SequenceNumber(150)]
        );
    }
}
