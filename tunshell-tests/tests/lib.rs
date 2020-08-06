use std::sync::Once;

static INIT: Once = Once::new();

/// Setup function that is only run once, even if called multiple times.
#[test]
fn setup() {
    INIT.call_once(env_logger::init);
}

pub mod utils;

pub mod valid_direct_connection;
pub mod valid_relay_connection;
