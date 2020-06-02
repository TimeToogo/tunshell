use dmp_client::Client;
use dmp_client::Config;
use std::process::exit;
use env_logger;
use log::debug;

#[tokio::main]
async fn main() -> () {
    env_logger::init();
    let config = Config::new_from_env();

    let result = Client::new(&config).start_session().await;

    match result {
        Ok(()) => exit(0),
        Err(err) => {
            debug!("Error occurred: {:?}", err);
            eprintln!("{}", err);
            exit(1)
        }
    }
}
