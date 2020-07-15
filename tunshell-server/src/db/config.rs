use anyhow::Result;
use mongodb::options::ClientOptions;
use std::env;

pub(crate) struct Config {
    pub(crate) connection_string: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self> {
        let connection_string = env::var("MONGO_CONNECTION_STRING")?;

        Ok(Self { connection_string })
    }

    pub(crate) async fn to_client_options(self) -> Result<ClientOptions> {
        let mut options = ClientOptions::parse(self.connection_string.as_ref()).await?;
        options.app_name = Some("Tunshell Server".to_owned());

        Ok(options)
    }
}
