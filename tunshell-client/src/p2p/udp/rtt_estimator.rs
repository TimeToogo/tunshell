use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct RttEstimator {
    /// The current estimated round trip time of the connection.
    estimate: Duration,

    /// Stores when packets were sent.
    /// The key is the sequence number of the packet the value is when it was sent.
    send_times: HashMap<u32, Instant>,
}

impl RttEstimator {
    pub(super) fn new() -> Self {
        Self {
            estimate: Duration::from_millis(0),
            send_times: HashMap::new(),
        }
    }
}
