use anyhow::Result;
use log::*;
use rusqlite::{params, Connection};

pub(crate) fn init(con: &mut Connection) -> Result<()> {
    info!("initialising sqlite schema");

    con.execute(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            peer1_key TEXT NOT NULL,
            peer2_key TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        ",
        params![],
    )?;

    con.execute(
        "
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_peer1_key ON
        sessions (peer1_key)
        ",
        params![],
    )?;

    con.execute(
        "
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_peer2_key ON
        sessions (peer2_key)
        ",
        params![],
    )?;

    info!("schema initialised");

    Ok(())
}
