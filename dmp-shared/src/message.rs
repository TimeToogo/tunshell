use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub struct RawMessage {
    type_id: u8,
    length: u16,
    data: Vec<u8>,
}

pub trait Message<T>: Unpin + Sync + Send {
    fn type_id(&self) -> u8;
    fn serialise(&self) -> Result<RawMessage>;
    fn deserialise(raw_message: &RawMessage) -> Result<T>;
}

#[derive(Debug, PartialEq)]
pub enum ServerMessage {
    Close,
    KeyAccepted(KeyAcceptedPayload),
    KeyRejected,
    PeerJoined(PeerJoinedPayload),
    PeerLeft,
    TimePlease,
    AttemptDirectConnect(AttemptDirectConnectPayload),
    StartRelayMode,
    Relay(RelayPayload),
}

#[derive(Debug, PartialEq)]
pub enum ClientMessage {
    Close,
    Key(KeyPayload),
    Time(TimePayload),
    DirectConnectSucceeded,
    DirectConnectFailed,
    Relay(RelayPayload),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum KeyType {
    #[serde(rename = "client")]
    Client,
    #[serde(rename = "host")]
    Host,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyAcceptedPayload {
    pub key_type: KeyType,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerJoinedPayload {
    pub peer_key: String,
    pub peer_ip_address: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptDirectConnectPayload {
    pub connect_at: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyPayload {
    pub key: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TimePayload {
    pub time: u64,
}

#[derive(Debug, PartialEq)]
pub struct RelayPayload {
    pub data: Vec<u8>,
}

impl RawMessage {
    pub fn new(type_id: u8, data: Vec<u8>) -> Self {
        assert!(data.len() <= i16::MAX as usize);

        Self {
            type_id,
            length: data.len() as u16,
            data,
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut vec = Vec::with_capacity(3 + self.data.len());
        vec.push(self.type_id);
        vec.push(((self.data.len() & 0xFF00) >> 8) as u8);
        vec.push((self.data.len() & 0xFF) as u8);
        vec.extend_from_slice(self.data.as_slice());

        vec
    }
}

impl Message<ServerMessage> for ServerMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::Close => 0,
            Self::KeyAccepted(_) => 1,
            Self::KeyRejected => 2,
            Self::PeerJoined(_) => 3,
            Self::PeerLeft => 4,
            Self::TimePlease => 5,
            Self::AttemptDirectConnect(_) => 6,
            Self::StartRelayMode => 7,
            Self::Relay(_) => 8,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let type_id = self.type_id();

        let data: Vec<u8> = match self {
            Self::Close => vec![],
            Self::KeyAccepted(payload) => serde_json::to_vec(&payload)?,
            Self::KeyRejected => vec![],
            Self::PeerJoined(payload) => serde_json::to_vec(&payload)?,
            Self::PeerLeft => vec![],
            Self::TimePlease => vec![],
            Self::AttemptDirectConnect(payload) => serde_json::to_vec(&payload)?,
            Self::StartRelayMode => vec![],
            Self::Relay(payload) => payload.data.clone(),
        };

        Ok(RawMessage::new(type_id, data))
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        match raw_message {
            RawMessage {
                type_id: 0,
                length: 0,
                data: _,
            } => Ok(Self::Close),
            RawMessage {
                type_id: 1,
                length: _,
                data,
            } => Ok(Self::KeyAccepted(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 2,
                length: 0,
                data: _,
            } => Ok(Self::KeyRejected),
            RawMessage {
                type_id: 3,
                length: _,
                data,
            } => Ok(Self::PeerJoined(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 4,
                length: 0,
                data: _,
            } => Ok(Self::PeerLeft),
            RawMessage {
                type_id: 5,
                length: 0,
                data: _,
            } => Ok(Self::TimePlease),
            RawMessage {
                type_id: 6,
                length: _,
                data,
            } => Ok(Self::AttemptDirectConnect(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 7,
                length: 0,
                data: _,
            } => Ok(Self::StartRelayMode),
            RawMessage {
                type_id: 8,
                length: _,
                data,
            } => Ok(Self::Relay(RelayPayload { data: data.clone() })),
            _ => Err(anyhow!("Failed to parse server message: {:?}", raw_message)),
        }
    }
}

impl Message<ClientMessage> for ClientMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::Close => 0,
            Self::Key(_) => 1,
            Self::Time(_) => 2,
            Self::DirectConnectSucceeded => 3,
            Self::DirectConnectFailed => 4,
            Self::Relay(_) => 5,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let type_id = self.type_id();

        let data: Vec<u8> = match self {
            Self::Close => vec![],
            Self::Key(payload) => serde_json::to_vec(&payload)?,
            Self::Time(payload) => serde_json::to_vec(&payload)?,
            Self::DirectConnectSucceeded => vec![],
            Self::DirectConnectFailed => vec![],
            Self::Relay(payload) => payload.data.clone(),
        };

        Ok(RawMessage::new(type_id, data))
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        match raw_message {
            RawMessage {
                type_id: 0,
                length: 0,
                data: _,
            } => Ok(Self::Close),
            RawMessage {
                type_id: 1,
                length: _,
                data,
            } => Ok(Self::Key(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 2,
                length: _,
                data,
            } => Ok(Self::Time(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 3,
                length: 0,
                data: _,
            } => Ok(Self::DirectConnectSucceeded),
            RawMessage {
                type_id: 4,
                length: 0,
                data: _,
            } => Ok(Self::DirectConnectFailed),
            RawMessage {
                type_id: 5,
                length: _,
                data,
            } => Ok(Self::Relay(RelayPayload { data: data.clone() })),
            _ => Err(anyhow!("Failed to parse client message: {:?}", raw_message)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_message_to_vec() {
        let raw_message = RawMessage::new(0, vec![1, 2, 3]);

        let vec = raw_message.to_vec();

        assert_eq!(vec, vec![0, 0, 3, 1, 2, 3]);
    }

    #[test]
    fn test_server_serialise_close() {
        let message = ServerMessage::Close;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 0);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_key_accepted() {
        let message = ServerMessage::KeyAccepted(KeyAcceptedPayload {
            key_type: KeyType::Client,
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 1);
        assert_eq!(
            String::from_utf8(raw_message.data).unwrap(),
            r#"{"keyType":"client"}"#
        );
        assert_eq!(raw_message.length, 20);
    }

    #[test]
    fn test_server_serialise_key_rejected() {
        let message = ServerMessage::KeyRejected;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 2);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_peer_joined() {
        let message = ServerMessage::PeerJoined(PeerJoinedPayload {
            peer_key: "key".to_string(),
            peer_ip_address: "123.123.123.123".to_string(),
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 3);
        assert_eq!(
            String::from_utf8(raw_message.data).unwrap(),
            r#"{"peerKey":"key","peerIpAddress":"123.123.123.123"}"#
        );
        assert_eq!(raw_message.length, 51);
    }

    #[test]
    fn test_server_serialise_peer_left() {
        let message = ServerMessage::PeerLeft;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 4);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_time_please() {
        let message = ServerMessage::TimePlease;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 5);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_attempt_direct_connect() {
        let message =
            ServerMessage::AttemptDirectConnect(AttemptDirectConnectPayload { connect_at: 12345 });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 6);
        assert_eq!(
            String::from_utf8(raw_message.data).unwrap(),
            r#"{"connectAt":12345}"#
        );
        assert_eq!(raw_message.length, 19);
    }

    #[test]
    fn test_server_serialise_start_relay_mode() {
        let message = ServerMessage::StartRelayMode;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 7);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_relay() {
        let message = ServerMessage::Relay(RelayPayload {
            data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 8);
        assert_eq!(raw_message.data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(raw_message.length, 9);
    }

    #[test]
    fn test_server_deserialise_close() {
        let raw_message = RawMessage::new(0, vec![]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::Close);
    }

    #[test]
    fn test_server_deserialise_key_accepted() {
        let raw_message = RawMessage::new(1, Vec::from(r#"{"keyType": "host"}"#.as_bytes()));

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::KeyAccepted(KeyAcceptedPayload {
                key_type: KeyType::Host
            })
        );
    }

    #[test]
    fn test_server_deserialise_key_rejected() {
        let raw_message = RawMessage::new(2, vec![]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::KeyRejected);
    }

    #[test]
    fn test_server_deserialise_peer_joined() {
        let raw_message = RawMessage::new(
            3,
            Vec::from(r#"{"peerKey": "key", "peerIpAddress": "123.123.123.123"}"#.as_bytes()),
        );

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: "key".to_owned(),
                peer_ip_address: "123.123.123.123".to_owned()
            })
        );
    }

    #[test]
    fn test_server_deserialise_peer_left() {
        let raw_message = RawMessage::new(4, vec![]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::PeerLeft);
    }

    #[test]
    fn test_server_deserialise_time_please() {
        let raw_message = RawMessage::new(5, vec![]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::TimePlease);
    }

    #[test]
    fn test_server_deserialise_attempt_direct_connect() {
        let raw_message = RawMessage::new(6, Vec::from(r#"{"connectAt": 12345}"#.as_bytes()));

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::AttemptDirectConnect(AttemptDirectConnectPayload { connect_at: 12345 })
        )
    }

    #[test]
    fn test_server_deserialise_start_relay_mode() {
        let raw_message = RawMessage::new(7, vec![]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::StartRelayMode);
    }

    #[test]
    fn test_server_deserialise_relay() {
        let raw_message = RawMessage::new(8, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
            })
        );
    }

    #[test]
    fn test_client_serialise_close() {
        let message = ClientMessage::Close;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 0);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_client_serialise_key() {
        let message = ClientMessage::Key(KeyPayload {
            key: "key".to_owned(),
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 1);
        assert_eq!(raw_message.data, Vec::from(r#"{"key":"key"}"#.as_bytes()));
        assert_eq!(raw_message.length, 13);
    }

    #[test]
    fn test_client_serialise_time() {
        let message = ClientMessage::Time(TimePayload { time: 12345 });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 2);
        assert_eq!(raw_message.data, Vec::from(r#"{"time":12345}"#.as_bytes()));
        assert_eq!(raw_message.length, 14);
    }

    #[test]
    fn test_client_serialise_direct_connect_succeeded() {
        let message = ClientMessage::DirectConnectSucceeded;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 3);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_client_serialise_direct_connect_failed() {
        let message = ClientMessage::DirectConnectFailed;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 4);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_client_serialise_relay() {
        let message = ClientMessage::Relay(RelayPayload {
            data: vec![1, 2, 3, 4, 5],
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 5);
        assert_eq!(raw_message.data, vec![1, 2, 3, 4, 5]);
        assert_eq!(raw_message.length, 5);
    }

    #[test]
    fn test_client_deserialise_close() {
        let raw_message = RawMessage::new(0, vec![]);

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::Close);
    }

    #[test]
    fn test_client_deserialise_key() {
        let raw_message = RawMessage::new(1, Vec::from(r#"{"key":"key"}"#.as_bytes()));

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ClientMessage::Key(KeyPayload {
                key: "key".to_owned()
            })
        );
    }

    #[test]
    fn test_client_deserialise_time() {
        let raw_message = RawMessage::new(2, Vec::from(r#"{"time":12345}"#.as_bytes()));

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::Time(TimePayload { time: 12345 }));
    }

    #[test]
    fn test_client_deserialise_direct_contact_succeeded() {
        let raw_message = RawMessage::new(3, vec![]);

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::DirectConnectSucceeded);
    }

    #[test]
    fn test_client_deserialise_direct_contact_failed() {
        let raw_message = RawMessage::new(4, vec![]);

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::DirectConnectFailed);
    }

    #[test]
    fn test_client_deserialise_relay() {
        let raw_message = RawMessage::new(5, vec![1, 2, 3, 4, 5]);

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ClientMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5]
            })
        );
    }
}
