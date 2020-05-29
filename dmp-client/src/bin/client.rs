use dmp_client::Client;
use dmp_client::Config;
use std::env;
use std::process::exit;

#[tokio::main]
async fn main() -> () {
    let config = Config::new_from_env();

    let result = Client::new(&config).start_session().await;

    match result {
        Ok(()) => exit(0),
        Err(err) => {
            if env::var("DMP_VERSION").is_ok() {
                println!("Error occurred: {:?}", err);
            }
            
            exit(1)
        }
    }
}
