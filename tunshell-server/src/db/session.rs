use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use mongodb::bson::{self, doc, oid::ObjectId};
use mongodb::{
    options::{FindOneOptions, UpdateOptions},
    Client,
};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::convert::TryFrom;

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Participant {
    pub(crate) key: String,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Session {
    id: ObjectId,
    pub(crate) peer1: Participant,
    pub(crate) peer2: Participant,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct SessionStore {
    client: Client,
}

impl Participant {
    pub(crate) fn new(key: String) -> Self {
        Self { key }
    }

    pub(crate) fn default() -> Self {
        Self::new(generate_secure_key())
    }
}

impl Into<bson::Document> for Participant {
    fn into(self) -> bson::Document {
        doc! {
            "key": self.key,
        }
    }
}

impl TryFrom<bson::Document> for Participant {
    type Error = Error;

    fn try_from(value: bson::Document) -> Result<Self, Self::Error> {
        let key = value.get_str("key")?.to_owned();
        Ok(Self { key })
    }
}

impl Into<bson::Document> for Session {
    fn into(self) -> bson::Document {
        doc! {
            "_id": self.id,
            "peer1": Into::<bson::Document>::into(self.peer1),
            "peer2": Into::<bson::Document>::into(self.peer2),
            "created_at": self.created_at
        }
    }
}

impl TryFrom<bson::Document> for Session {
    type Error = Error;

    fn try_from(value: bson::Document) -> Result<Self, Self::Error> {
        let id = value.get_object_id("_id")?.clone();
        let peer1 = Participant::try_from(value.get_document("peer1")?.clone())?;
        let peer2 = Participant::try_from(value.get_document("peer2")?.clone())?;
        let created_at = value.get_datetime("created_at")?.clone();

        Ok(Self {
            id,
            peer1,
            peer2,
            created_at,
        })
    }
}

impl Session {
    pub(crate) fn new(peer1: Participant, peer2: Participant) -> Session {
        Session {
            id: ObjectId::new(),
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
    pub(crate) fn new(client: Client) -> SessionStore {
        SessionStore { client }
    }

    pub(crate) async fn find_by_key(&mut self, key: &str) -> Result<Option<Session>> {
        //  $or: [{ 'host.key': key }, { 'client.key': key }],
        let result = self
            .client
            .database("relay")
            .collection("sessions")
            .find_one(
                doc! {
                    "$or": [{"peer1.key": key.clone()}, {"peer2.key": key.clone()}]
                },
                FindOneOptions::default(),
            )
            .await?;

        let doc = match result {
            None => return Ok(None),
            Some(doc) => doc,
        };

        Ok(Some(Session::try_from(doc)?))
    }

    pub(crate) async fn save(&mut self, session: &Session) -> Result<()> {
        let result = self
            .client
            .database("relay")
            .collection("sessions")
            .update_one(
                doc! { "_id": session.id.clone() },
                doc! {
                    "$set": Into::<bson::Document>::into(session.clone())
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await;

        if let Err(err) = result {
            return Err(Error::from(err));
        }

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

    #[test]
    fn test_participant_into_bson() {
        let participant = Participant::new("test-key".to_owned());

        let document: bson::Document = participant.into();

        assert_eq!(
            document,
            doc! {
                "key": "test-key",
            }
        );
    }

    #[test]
    fn test_participant_from_bson() {
        let document = doc! {
            "key": "test-key",
        };

        let participant = Participant::try_from(document).unwrap();

        assert_eq!(participant, Participant::new("test-key".to_owned()));
    }

    #[test]
    fn test_session_into_bson() {
        let session = Session::new(
            Participant::new("peer1-key".to_owned()),
            Participant::new("peer2-key".to_owned()),
        );

        let document: bson::Document = session.clone().into();

        assert_eq!(
            document,
            doc! {
                "_id": session.id,
                "peer1": {
                    "key": "peer1-key",
                },
                "peer2": {
                    "key": "peer2-key",
                },
                "created_at": session.created_at
            }
        );
    }

    #[test]
    fn test_session_from_bson() {
        let id = ObjectId::new();
        let created_at = Utc::now();

        let document = doc! {
            "_id": id.clone(),
            "peer1": {
                "key": "peer1-key",
            },
            "peer2": {
                "key": "peer2-key",
            },
            "created_at": created_at.clone()
        };

        let session = Session::try_from(document).unwrap();

        let mut expected = Session::new(
            Participant::new("peer1-key".to_owned()),
            Participant::new("peer2-key".to_owned()),
        );

        expected.id = id;
        expected.created_at = created_at;

        assert_eq!(session, expected);
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
