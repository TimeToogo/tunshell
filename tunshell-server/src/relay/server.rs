use super::{config::Config, connection::Connection};
use anyhow::Result;
use log::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;

pub(super) struct Server {
    config: Config,
    pending_connections: Arc<Mutex<Vec<JoinHandle<Result<Connection>>>>>,
    waiting_connections: Arc<Mutex<HashMap<String, Connection>>>,
    paired_connections: Arc<Mutex<Vec<JoinHandle<(Connection, Connection)>>>>,
}
