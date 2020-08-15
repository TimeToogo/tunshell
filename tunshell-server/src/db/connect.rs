use super::{schema, Config};
use anyhow::{Context, Error, Result};
use log::*;
use rusqlite::Connection;

pub(crate) async fn connect() -> Result<Connection> {
    tokio::task::spawn_blocking(connect_sync)
        .await
        .context("error while connecting to sqlite")?
}

fn connect_sync() -> Result<Connection> {
    info!("connecting to sqlite");
    let config = Config::from_env()?;

    let mut con = match Connection::open(config.sqlite_db_path) {
        Ok(con) => con,
        Err(err) => {
            error!("failed to connect to sqlite: {}", err);
            return Err(Error::from(err));
        }
    };

    info!("connected to sqlite");

    schema::init(&mut con).context("error while initialising sqlite schema")?;

    Ok(con)
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
