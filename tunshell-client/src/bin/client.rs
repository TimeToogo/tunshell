use tunshell_client::Client;
use tunshell_client::Config;
use env_logger;
use log::error;
use std::process::exit;

#[tokio::main]
async fn main() -> () {
    env_logger::init();

    let config = Config::new_from_env();

    let result = Client::new(&config).start_session().await;

    match result {
        Ok(code) => exit(code as i32),
        Err(err) => {
            error!("Error occurred: {:?}", err);
            eprintln!("{}", err);
            exit(1)
        }
    }
}
