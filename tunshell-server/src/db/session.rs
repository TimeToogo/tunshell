use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use rusqlite::{named_params, params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Participant {
    pub(crate) key: String,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Session {
    id: String,
    pub(crate) peer1: Participant,
    pub(crate) peer2: Participant,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct SessionStore {
    con: Arc<Mutex<Connection>>,
}

impl Participant {
    pub(crate) fn new(key: String) -> Self {
        Self { key }
    }

    pub(crate) fn default() -> Self {
        Self::new(generate_secure_key())
    }
}

impl Session {
    pub(crate) fn new(peer1: Participant, peer2: Participant) -> Session {
        Session {
            id: Uuid::new_v4().to_string(),
            peer1,
            peer2,
            created_at: Utc::now(),
        }
    }

    pub(crate) fn participant(&self, key: &str) -> Option<&Participant> {
        if self.peer1.key == key {
            return Some(&self.peer1);
        }

        if self.peer2.key == key {
            return Some(&self.peer2);
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn participant_mut(&mut self, key: &str) -> Option<&mut Participant> {
        if self.peer1.key == key {
            return Some(&mut self.peer1);
        }

        if self.peer2.key == key {
            return Some(&mut self.peer2);
        }

        None
    }

    pub(crate) fn other_participant(&self, key: &str) -> Option<&Participant> {
        if self.peer1.key == key {
            return Some(&self.peer2);
        }

        if self.peer2.key == key {
            return Some(&self.peer1);
        }

        None
    }
}

impl SessionStore {
    pub(crate) fn new(con: Connection) -> SessionStore {
        SessionStore {
            con: Arc::new(Mutex::new(con)),
        }
    }

    pub(crate) async fn find_by_key(&mut self, key: &str) -> Result<Option<Session>> {
        let con = Arc::clone(&self.con);
        let key = key.to_owned();

        tokio::task::spawn_blocking(move || {
            let con = con.lock().unwrap();

            Self::find_by_key_sync(&con, key.as_str())
        })
        .await
        .context("error while finding session by key")?
    }

    fn find_by_key_sync(con: &Connection, key: &str) -> Result<Option<Session>> {
        let mut statement = con.prepare(
            "
            SELECT id, peer1_key, peer2_key, created_at FROM sessions
            WHERE peer1_key = :key OR peer2_key = :key
        ",
        )?;

        let mut result = statement.query_named(named_params! {":key": key})?;

        let row = match result.next()? {
            Some(row) => row,
            None => return Ok(None),
        };

        let session = Session {
            id: row.get(0)?,
            peer1: Participant { key: row.get(1)? },
            peer2: Participant { key: row.get(2)? },
            created_at: DateTime::parse_from_rfc3339(row.get::<usize, String>(3)?.as_str())?
                .with_timezone(&Utc),
        };

        Ok(Some(session))
    }

    pub(crate) async fn save(&mut self, session: &Session) -> Result<()> {
        let con = Arc::clone(&self.con);
        let session = session.clone();

        tokio::task::spawn_blocking(move || {
            let con = con.lock().unwrap();

            Self::save_sync(&con, &session)
        })
        .await
        .context("error while saving session")??;

        Ok(())
    }

    fn save_sync(con: &Connection, session: &Session) -> Result<()> {
        con.execute(
            "
                INSERT OR REPLACE INTO sessions (id, peer1_key, peer2_key, created_at)
                VALUES (?1, ?2, ?3, ?4)
            ",
            params![
                session.id,
                session.peer1.key,
                session.peer2.key,
                session.created_at.to_rfc3339()
            ],
        )?;

        Ok(())
    }
}

// Generates ~131 bits of entropy (22 chars) using alphanumeric charset
pub(crate) fn generate_secure_key() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(22).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tokio::runtime::Runtime;

    #[test]
    fn test_find_by_key() {
        Runtime::new().unwrap().block_on(async {
            let mut store = SessionStore::new(db::connect().await.unwrap());

            {
                let con = store.con.lock().unwrap();

                con.execute(
                    r#"
                    INSERT OR REPLACE INTO sessions (id, peer1_key, peer2_key, created_at)
                    VALUES ("test_id", "valid_peer1_key", "valid_peer2_key", "2000-01-01T01:01:01.000Z")
                    "#,
                    params![],
                ).unwrap();
            }

            let session = Session {
                id: "test_id".to_owned(),
                peer1: Participant { key: "valid_peer1_key".to_owned() },
                peer2: Participant { key: "valid_peer2_key".to_owned() },
                created_at: DateTime::parse_from_rfc3339("2000-01-01T01:01:01.000Z").unwrap().with_timezone(&Utc)
            };

            assert_eq!(store.find_by_key("valid_peer1_key").await.unwrap(), Some(session.clone()));
            assert_eq!(store.find_by_key("valid_peer2_key").await.unwrap(), Some(session.clone())); 
            assert_eq!(store.find_by_key("invalid_key").await.unwrap(), None); 
        });
    }

    #[test]
    fn test_save() {
        Runtime::new().unwrap().block_on(async {
            let mut store = SessionStore::new(db::connect().await.unwrap());

            let session = Session {
                id: "test_save_id".to_owned(),
                peer1: Participant {
                    key: "valid_peer1_key".to_owned(),
                },
                peer2: Participant {
                    key: "valid_peer2_key".to_owned(),
                },
                created_at: DateTime::parse_from_rfc3339("2000-01-01T01:01:01.000Z")
                    .unwrap()
                    .with_timezone(&Utc),
            };

            store.save(&session).await.unwrap();

            let count: u32 = {
                let con = store.con.lock().unwrap();

                con.query_row(
                    r#"
                    SELECT COUNT(*) FROM sessions
                    WHERE 1=1
                    AND id = "test_save_id"
                    AND peer1_key = "valid_peer1_key"
                    AND peer2_key = "valid_peer2_key"
                    AND created_at = "2000-01-01T01:01:01+00:00"
                    "#,
                    params![],
                    |r| r.get(0),
                )
                .unwrap()
            };

            assert_eq!(count, 1);
        });
    }

    #[test]
    fn test_generate_secure_id() {
        let id1 = generate_secure_key();
        let id2 = generate_secure_key();

        assert_eq!(id1.len(), 22);
        assert_eq!(id2.len(), 22);
        assert_ne!(id1, id2);
    }
}
