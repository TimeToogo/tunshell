use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use mongodb::bson::{self, doc, oid::ObjectId};
use mongodb::{
    options::{FindOneOptions, UpdateOptions},
    Client,
};
use std::convert::TryFrom;
use tunshell_shared::KeyType;

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Participant {
    pub(crate) key: String,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Session {
    id: ObjectId,
    pub(crate) host: Participant,
    pub(crate) client: Participant,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct SessionStore {
    client: Client,
}

impl Participant {
    pub(crate) fn new(key: String) -> Participant {
        Participant { key }
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
            "host": Into::<bson::Document>::into(self.host),
            "client": Into::<bson::Document>::into(self.client),
            "created_at": self.created_at
        }
    }
}

impl TryFrom<bson::Document> for Session {
    type Error = Error;

    fn try_from(value: bson::Document) -> Result<Self, Self::Error> {
        let id = value.get_object_id("_id")?.clone();
        let host = Participant::try_from(value.get_document("host")?.clone())?;
        let client = Participant::try_from(value.get_document("client")?.clone())?;
        let created_at = value.get_datetime("created_at")?.clone();

        Ok(Self {
            id,
            host,
            client,
            created_at,
        })
    }
}

impl Session {
    pub(crate) fn new(host: Participant, client: Participant) -> Session {
        Session {
            id: ObjectId::new(),
            host,
            client,
            created_at: Utc::now(),
        }
    }

    pub(crate) fn participant(&self, key: &str) -> Option<&Participant> {
        if self.host.key == key {
            return Some(&self.host);
        }

        if self.client.key == key {
            return Some(&self.client);
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn participant_mut(&mut self, key: &str) -> Option<&mut Participant> {
        if self.host.key == key {
            return Some(&mut self.host);
        }

        if self.client.key == key {
            return Some(&mut self.client);
        }

        None
    }

    pub(crate) fn other_participant(&self, key: &str) -> Option<&Participant> {
        if self.host.key == key {
            return Some(&self.client);
        }

        if self.client.key == key {
            return Some(&self.host);
        }

        None
    }

    pub(crate) fn key_type(&self, key: &str) -> Option<KeyType> {
        if self.host.key == key {
            return Some(KeyType::Host);
        }

        if self.client.key == key {
            return Some(KeyType::Client);
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
                    "$or": [{"host.key": key.clone()}, {"client.key": key.clone()}]
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
            Participant::new("host-key".to_owned()),
            Participant::new("client-key".to_owned()),
        );

        let document: bson::Document = session.clone().into();

        assert_eq!(
            document,
            doc! {
                "_id": session.id,
                "host": {
                    "key": "host-key",
                },
                "client": {
                    "key": "client-key",
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
            "host": {
                "key": "host-key",
            },
            "client": {
                "key": "client-key",
            },
            "created_at": created_at.clone()
        };

        let session = Session::try_from(document).unwrap();

        let mut expected = Session::new(
            Participant::new("host-key".to_owned()),
            Participant::new("client-key".to_owned()),
        );

        expected.id = id;
        expected.created_at = created_at;

        assert_eq!(session, expected);
    }
}
