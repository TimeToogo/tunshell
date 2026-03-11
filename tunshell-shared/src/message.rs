use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone)]
pub struct RawMessage {
    type_id: u8,
    length: u16,
    data: Vec<u8>,
}

pub trait Message: Unpin + Sync + Send + std::fmt::Debug
where
    Self: Sized,
{
    fn type_id(&self) -> u8;
    fn serialise(&self) -> Result<RawMessage>;
    fn deserialise(raw_message: &RawMessage) -> Result<Self>;
}

#[derive(Debug, PartialEq, Clone)]
pub enum ServerMessage {
    Close,
    KeyAccepted,
    KeyRejected,
    AlreadyJoined,
    PeerJoined(PeerJoinedPayload),
    PeerLeft,
    BindForDirectConnect,
    AttemptDirectConnect(PortBindings),
    StartRelayMode,
    Relay(RelayPayload),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ClientMessage {
    Close,
    Key(KeyPayload),
    DirectConnectBound(PortBindings),
    DirectConnectSucceeded,
    DirectConnectFailed,
    Relay(RelayPayload),
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct PeerJoinedPayload {
    pub peer_key: String,
    pub peer_ip_address: String,
    pub session_nonce: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct PortBindings {
    pub udp_port: Option<u16>,
    pub tcp_port: Option<u16>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct KeyPayload {
    pub key: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct RelayPayload {
    pub data: Vec<u8>,
}

impl RawMessage {
    pub fn new(type_id: u8, data: Vec<u8>) -> Result<Self> {
        if data.len() > i16::MAX as usize {
            return Err(Error::msg(format!(
                "message length ({}) cannot be greater than {}",
                data.len(),
                i16::MAX
            )));
        }
        assert!(data.len() <= i16::MAX as usize);

        Ok(Self {
            type_id,
            length: data.len() as u16,
            data,
        })
    }

    pub fn type_id(&self) -> u8 {
        self.type_id
    }

    pub fn data(&self) -> &Vec<u8> {
        &self.data
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

impl Message for ServerMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::Close => 0,
            Self::KeyAccepted => 1,
            Self::KeyRejected => 2,
            Self::AlreadyJoined => 3,
            Self::PeerJoined(_) => 4,
            Self::PeerLeft => 5,
            Self::BindForDirectConnect => 6,
            Self::AttemptDirectConnect(_) => 7,
            Self::StartRelayMode => 8,
            Self::Relay(_) => 9,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let type_id = self.type_id();

        let data: Vec<u8> = match self {
            Self::Close => vec![],
            Self::KeyAccepted => vec![],
            Self::KeyRejected => vec![],
            Self::AlreadyJoined => vec![],
            Self::PeerJoined(payload) => serde_json::to_vec(&payload)?,
            Self::PeerLeft => vec![],
            Self::BindForDirectConnect => vec![],
            Self::AttemptDirectConnect(payload) => serde_json::to_vec(&payload)?,
            Self::StartRelayMode => vec![],
            Self::Relay(payload) => payload.data.clone(),
        };

        RawMessage::new(type_id, data)
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
                data: _,
            } => Ok(Self::KeyAccepted),
            RawMessage {
                type_id: 2,
                length: 0,
                data: _,
            } => Ok(Self::KeyRejected),
            RawMessage {
                type_id: 3,
                length: 0,
                data: _,
            } => Ok(Self::AlreadyJoined),
            RawMessage {
                type_id: 4,
                length: _,
                data,
            } => Ok(Self::PeerJoined(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 5,
                length: 0,
                data: _,
            } => Ok(Self::PeerLeft),
            RawMessage {
                type_id: 6,
                length: _,
                data: _,
            } => Ok(Self::BindForDirectConnect),
            RawMessage {
                type_id: 7,
                length: _,
                data,
            } => Ok(Self::AttemptDirectConnect(serde_json::from_slice(data)?)),
            RawMessage {
                type_id: 8,
                length: 0,
                data: _,
            } => Ok(Self::StartRelayMode),
            RawMessage {
                type_id: 9,
                length: _,
                data,
            } => Ok(Self::Relay(RelayPayload { data: data.clone() })),
            _ => Err(anyhow!("Failed to parse server message: {:?}", raw_message)),
        }
    }
}

impl Message for ClientMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::Close => 0,
            Self::Key(_) => 1,
            Self::DirectConnectBound(_) => 2,
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
            Self::DirectConnectBound(payload) => serde_json::to_vec(&payload)?,
            Self::DirectConnectSucceeded => vec![],
            Self::DirectConnectFailed => vec![],
            Self::Relay(payload) => payload.data.clone(),
        };

        RawMessage::new(type_id, data)
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
            } => Ok(Self::DirectConnectBound(serde_json::from_slice(data)?)),
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
        let raw_message = RawMessage::new(0, vec![1, 2, 3]).unwrap();

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
        let message = ServerMessage::KeyAccepted;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 1);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
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
    fn test_server_serialise_already_joined() {
        let message = ServerMessage::AlreadyJoined;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 3);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_peer_joined() {
        let message = ServerMessage::PeerJoined(PeerJoinedPayload {
            peer_key: "key".to_string(),
            peer_ip_address: "123.123.123.123".to_string(),
            session_nonce: "nonce".to_string(),
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 4);
        assert_eq!(
            String::from_utf8(raw_message.data.clone()).unwrap(),
            r#"{"peer_key":"key","peer_ip_address":"123.123.123.123","session_nonce":"nonce"}"#
        );
        assert_eq!(raw_message.length, raw_message.data.len() as u16);
    }

    #[test]
    fn test_server_serialise_peer_left() {
        let message = ServerMessage::PeerLeft;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 5);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_bind_for_direct_connect() {
        let message = ServerMessage::BindForDirectConnect;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 6);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_attempt_direct_connect() {
        let message = ServerMessage::AttemptDirectConnect(PortBindings {
            tcp_port: Some(1234),
            udp_port: Some(2222),
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 7);
        assert_eq!(
            String::from_utf8(raw_message.data.clone()).unwrap(),
            r#"{"udp_port":2222,"tcp_port":1234}"#
        );
        assert_eq!(raw_message.length, raw_message.data.len() as u16);
    }

    #[test]
    fn test_server_serialise_attempt_direct_connect_with_nulls() {
        let message = ServerMessage::AttemptDirectConnect(PortBindings {
            tcp_port: None,
            udp_port: None,
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 7);
        assert_eq!(
            String::from_utf8(raw_message.data.clone()).unwrap(),
            r#"{"udp_port":null,"tcp_port":null}"#
        );
        assert_eq!(raw_message.length, raw_message.data.len() as u16);
    }

    #[test]
    fn test_server_serialise_start_relay_mode() {
        let message = ServerMessage::StartRelayMode;

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 8);
        assert_eq!(raw_message.data.len(), 0);
        assert_eq!(raw_message.length, 0);
    }

    #[test]
    fn test_server_serialise_relay() {
        let message = ServerMessage::Relay(RelayPayload {
            data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 9);
        assert_eq!(raw_message.data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(raw_message.length, 9);
    }

    #[test]
    fn test_server_deserialise_close() {
        let raw_message = RawMessage::new(0, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::Close);
    }

    #[test]
    fn test_server_deserialise_key_accepted() {
        let raw_message = RawMessage::new(1, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::KeyAccepted);
    }

    #[test]
    fn test_server_deserialise_key_rejected() {
        let raw_message = RawMessage::new(2, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::KeyRejected);
    }

    #[test]
    fn test_server_deserialise_already_joined() {
        let raw_message = RawMessage::new(3, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::AlreadyJoined);
    }

    #[test]
    fn test_server_deserialise_peer_joined() {
        let raw_message = RawMessage::new(
            4,
            Vec::from(r#"{"peer_key": "key", "peer_ip_address": "123.123.123.123", "session_nonce": "nonce"}"#.as_bytes()),
        ).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: "key".to_owned(),
                peer_ip_address: "123.123.123.123".to_owned(),
                session_nonce: "nonce".to_owned()
            })
        );
    }

    #[test]
    fn test_server_deserialise_peer_left() {
        let raw_message = RawMessage::new(5, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::PeerLeft);
    }

    #[test]
    fn test_server_deserialise_bind_for_direct_connect() {
        let raw_message = RawMessage::new(6, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::BindForDirectConnect)
    }

    #[test]
    fn test_server_deserialise_attempt_direct_connect() {
        let raw_message = RawMessage::new(
            7,
            Vec::from(r#"{"tcp_port":12345,"udp_port":123}"#.as_bytes()),
        )
        .unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ServerMessage::AttemptDirectConnect(PortBindings {
                tcp_port: Some(12345),
                udp_port: Some(123),
            })
        )
    }

    #[test]
    fn test_server_deserialise_start_relay_mode() {
        let raw_message = RawMessage::new(8, vec![]).unwrap();

        let message = ServerMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ServerMessage::StartRelayMode);
    }

    #[test]
    fn test_server_deserialise_relay() {
        let raw_message = RawMessage::new(9, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();

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
    fn test_client_serialise_direct_connect_bound() {
        let message = ClientMessage::DirectConnectBound(PortBindings {
            udp_port: Some(4444),
            tcp_port: Some(5555),
        });

        let raw_message = message.serialise().unwrap();

        assert_eq!(raw_message.type_id, 2);
        assert_eq!(
            raw_message.data,
            Vec::from(r#"{"udp_port":4444,"tcp_port":5555}"#.as_bytes())
        );
        assert_eq!(raw_message.length, raw_message.data.len() as u16);
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
        let raw_message = RawMessage::new(0, vec![]).unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::Close);
    }

    #[test]
    fn test_client_deserialise_key() {
        let raw_message = RawMessage::new(1, Vec::from(r#"{"key":"key"}"#.as_bytes())).unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ClientMessage::Key(KeyPayload {
                key: "key".to_owned()
            })
        );
    }

    #[test]
    fn test_client_deserialise_direct_contact_bound() {
        let raw_message = RawMessage::new(
            2,
            Vec::from(r#"{"tcp_port":null,"udp_port":2222}"#.as_bytes()),
        )
        .unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ClientMessage::DirectConnectBound(PortBindings {
                tcp_port: None,
                udp_port: Some(2222)
            })
        );
    }

    #[test]
    fn test_client_deserialise_direct_contact_succeeded() {
        let raw_message = RawMessage::new(3, vec![]).unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::DirectConnectSucceeded);
    }

    #[test]
    fn test_client_deserialise_direct_contact_failed() {
        let raw_message = RawMessage::new(4, vec![]).unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(message, ClientMessage::DirectConnectFailed);
    }

    #[test]
    fn test_client_deserialise_relay() {
        let raw_message = RawMessage::new(5, vec![1, 2, 3, 4, 5]).unwrap();

        let message = ClientMessage::deserialise(&raw_message).unwrap();

        assert_eq!(
            message,
            ClientMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5]
            })
        );
    }
}
