use std::sync::Once;

static INIT: Once = Once::new();

#[test]
fn setup() {
    // TODO: ensure runs before all other tests
    INIT.call_once(env_logger::init);
}

pub mod utils;

pub mod valid_direct_connection;
pub mod valid_relay_connection;
