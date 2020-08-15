use anyhow::Result;
use std::env;

pub(crate) struct Config {
    pub(crate) sqlite_db_path: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self> {
        let sqlite_db_path = env::var("SQLITLE_DB_PATH")?;

        Ok(Self { sqlite_db_path })
    }
}
