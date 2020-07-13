use super::Config;
use anyhow::{Error, Result};
use log::*;
use mongodb::Client;

pub(crate) async fn connect() -> Result<Client> {
    info!("connecting to mongodb");
    let config = Config::from_env()?;
    let client_options = config.to_client_options().await?;

    match Client::with_options(client_options) {
        Ok(client) => {
            info!("connected to mongo");
            Ok(client)
        }
        Err(err) => {
            error!("failed to connect to mongo: {}", err);
            Err(Error::from(err))
        }
    }
}

#[cfg(all(test, integration))]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_connect() {
        Runtime::new().unwrap().block_on(async {
            let client = connect().await.unwrap();
            let names = client.list_database_names(None, None).await.unwrap();

            assert_eq!(names, vec!["relay"]);
        });
    }
}
