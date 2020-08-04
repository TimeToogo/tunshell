use anyhow::Result;
use env_logger;
use tokio;
use tunshell_server::{api, relay};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    tunshell_server::start(relay::Config::from_env()?).await
}
