use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::convert::From;
use tunshell_shared::{Message, MessageStream, RawMessage};

use crate::shell::network::NetworkPeerConfig;

#[derive(Debug, PartialEq, Clone)]
pub enum ShellClientMessage {
    Key(String),
    StartShell(StartShellPayload),
    Network(NetworkMessage),
    Stdin(Vec<u8>),
    Resize(WindowSize),
    RemotePtyData(RemotePtyDataPayload),
    Error(String),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ShellServerMessage {
    KeyAccepted,
    KeyRejected,
    ShellStarted(ShellStartedPayload),
    Network(NetworkMessage),
    Stdout(Vec<u8>),
    Exited(u8),
    RemotePtyEvent(RemotePtyEventPayload),
    Error(String),
}

pub type ConId = u32;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum NetworkMessage {
    TcpConnect(ConId, String, u16),
    TcpConnectResult(ConId, Option<String>),
    TcpSend(ConId, Vec<u8>),
    TcpRecv(ConId, Vec<u8>),
    TcpClose(ConId),
    UdpSend(ConId, Vec<u8>),
    UdpRecv(ConId, Vec<u8>),
    Notice(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(super) struct StartShellPayload {
    pub(super) term: String,
    pub(super) color: bool,
    pub(super) size: WindowSize,
    pub(super) remote_pty_support: bool,
    pub(super) network_peer_config: NetworkPeerConfig,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(super) struct WindowSize(pub(super) u16, pub(super) u16);

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(super) enum ShellStartedPayload {
    LocalPty,
    RemotePty,
    Fallback,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(super) enum RemotePtyEventPayload {
    Connect(u32),
    Payload(RemotePtyDataPayload),
    Close(u32),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(super) struct RemotePtyDataPayload {
    pub con_id: u32,
    pub data: Vec<u8>,
}

pub(super) type ShellClientStream<S> = MessageStream<ShellClientMessage, ShellServerMessage, S>;

#[cfg(not(target_arch = "wasm32"))]
pub(super) type ShellServerStream<S> = MessageStream<ShellServerMessage, ShellClientMessage, S>;

impl Message for ShellClientMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::Key(_) => 1,
            Self::StartShell(_) => 2,
            Self::Stdin(_) => 3,
            Self::Resize(_) => 4,
            Self::RemotePtyData(_) => 5,
            Self::Network(_) => 6,
            Self::Error(_) => 255,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let buff = match self {
            Self::Key(key) => key.as_bytes().to_vec(),
            Self::StartShell(payload) => serde_json::to_vec(&payload)?,
            Self::Stdin(payload) => payload.clone(),
            Self::Resize(payload) => serde_json::to_vec(&payload)?,
            Self::RemotePtyData(payload) => serde_json::to_vec(&payload)?,
            Self::Network(network) => network.serialise_embedded()?,
            Self::Error(payload) => payload.as_bytes().to_vec(),
        };

        RawMessage::new(self.type_id(), buff)
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        let parsed = match raw_message.type_id() {
            1 => Self::Key(String::from_utf8(raw_message.data().clone())?),
            2 => Self::StartShell(serde_json::from_slice(raw_message.data().as_slice())?),
            3 => Self::Stdin(raw_message.data().clone()),
            4 => Self::Resize(serde_json::from_slice(raw_message.data().as_slice())?),
            5 => Self::RemotePtyData(serde_json::from_slice(raw_message.data().as_slice())?),
            6 => Self::Network(NetworkMessage::deserialise_embedded(
                raw_message.data().as_slice(),
            )?),
            255 => Self::Error(String::from_utf8(raw_message.data().clone())?),
            id @ _ => {
                return Err(Error::msg(format!(
                    "Unknown type id for ShellClientMessage: {}",
                    id
                )))
            }
        };

        Ok(parsed)
    }
}

impl Message for ShellServerMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::KeyAccepted => 1,
            Self::KeyRejected => 2,
            Self::ShellStarted(_) => 3,
            Self::Stdout(_) => 4,
            Self::Exited(_) => 5,
            Self::RemotePtyEvent(_) => 6,
            Self::Network(_) => 7,
            Self::Error(_) => 255,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let buff = match self {
            Self::KeyAccepted => Vec::<u8>::new(),
            Self::KeyRejected => Vec::<u8>::new(),
            Self::ShellStarted(payload) => serde_json::to_vec(payload)?,
            Self::Stdout(payload) => payload.clone(),
            Self::Exited(payload) => vec![*payload],
            Self::RemotePtyEvent(payload) => serde_json::to_vec(payload)?,
            Self::Network(network) => network.serialise_embedded()?,
            Self::Error(payload) => payload.as_bytes().to_vec(),
        };

        RawMessage::new(self.type_id(), buff)
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        let message = match raw_message.type_id() {
            1 => Self::KeyAccepted,
            2 => Self::KeyRejected,
            3 => Self::ShellStarted(serde_json::from_slice(raw_message.data().as_slice())?),
            4 => Self::Stdout(raw_message.data().clone()),
            5 => Self::Exited(raw_message.data().get(0).map_or_else(
                || Err(Error::msg("encountered exit message without exit code")),
                |v| Ok(*v),
            )?),
            6 => Self::RemotePtyEvent(serde_json::from_slice(raw_message.data().as_slice())?),
            7 => Self::Network(NetworkMessage::deserialise_embedded(
                raw_message.data().as_slice(),
            )?),
            255 => Self::Error(String::from_utf8(raw_message.data().clone())?),
            id @ _ => {
                return Err(Error::msg(format!(
                    "Unknown type id for ShellServerMessage: {}",
                    id
                )))
            }
        };

        Ok(message)
    }
}

impl Message for NetworkMessage {
    fn type_id(&self) -> u8 {
        match self {
            Self::TcpConnect(_, _, _) => 3,
            Self::TcpConnectResult(_, _) => 4,
            Self::TcpSend(_, _) => 5,
            Self::TcpRecv(_, _) => 6,
            Self::TcpClose(_) => 7,
            Self::UdpRecv(_, _) => 10,
            Self::UdpSend(_, _) => 11,
            Self::Notice(_) => 255,
        }
    }

    fn serialise(&self) -> Result<RawMessage> {
        let buff = match self {
            Self::TcpSend(con_id, payload)
            | Self::TcpRecv(con_id, payload)
            | Self::UdpSend(con_id, payload) => {
                let mut buff = con_id.to_be_bytes().to_vec();
                buff.extend_from_slice(payload.as_slice());
                buff
            }
            _ => serde_json::to_vec(self)?,
        };

        RawMessage::new(self.type_id(), buff)
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        match raw_message.type_id() {
            5 => Self::deserialise_send_like(raw_message.data().as_slice(), Self::TcpSend),
            6 => Self::deserialise_send_like(raw_message.data().as_slice(), Self::TcpRecv),
            10 => Self::deserialise_send_like(raw_message.data().as_slice(), Self::UdpRecv),
            11 => Self::deserialise_send_like(raw_message.data().as_slice(), Self::UdpSend),
            _ => Ok(serde_json::from_slice(raw_message.data().as_slice())?),
        }
    }
}

impl NetworkMessage {
    fn serialise_embedded(&self) -> Result<Vec<u8>> {
        let raw_message = self.serialise()?;
        let mut buff = Vec::with_capacity(1 + raw_message.data().len());
        buff.push(raw_message.type_id());
        buff.extend_from_slice(raw_message.data().as_slice());
        Ok(buff)
    }

    fn deserialise_embedded(data: &[u8]) -> Result<Self> {
        let type_id = *data
            .first()
            .ok_or_else(|| Error::msg("encountered empty embedded network message"))?;

        Self::deserialise(&RawMessage::new(type_id, data[1..].to_vec())?)
    }

    fn deserialise_send_like<F>(data: &[u8], constructor: F) -> Result<Self>
    where
        F: FnOnce(ConId, Vec<u8>) -> Self,
    {
        if data.len() < 4 {
            return Err(Error::msg(
                "encountered truncated network send/recv message",
            ));
        }

        let con_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        Ok(constructor(con_id, data[4..].to_vec()))
    }
}

impl From<(u16, u16)> for WindowSize {
    fn from(size: (u16, u16)) -> Self {
        Self(size.0, size.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_serialise_key() {
        let message = ShellClientMessage::Key("key".to_owned());
        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised,
            RawMessage::new(1, "key".as_bytes().to_vec()).unwrap()
        );

        let deserialised = ShellClientMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_client_serialise_start_shell() {
        let payload = StartShellPayload {
            term: "test".to_owned(),
            color: true,
            size: WindowSize(100, 50),
            remote_pty_support: false,
            network_peer_config: NetworkPeerConfig::default(),
        };
        let message = ShellClientMessage::StartShell(payload.clone());
        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised,
            RawMessage::new(2, serde_json::to_vec(&payload).unwrap()).unwrap()
        );

        let deserialised = ShellClientMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_client_serialise_stdin() {
        let message = ShellClientMessage::Stdin(vec![1, 2, 3, 4, 5]);
        let serialised = message.serialise().unwrap();

        assert_eq!(serialised, RawMessage::new(3, vec![1, 2, 3, 4, 5]).unwrap());

        let deserialised = ShellClientMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_client_serialise_resize() {
        let message = ShellClientMessage::Resize(WindowSize(50, 100));
        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised,
            RawMessage::new(4, "[50,100]".as_bytes().to_vec()).unwrap()
        );

        let deserialised = ShellClientMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_key_accepted() {
        let message = ShellServerMessage::KeyAccepted;
        let serialised = message.serialise().unwrap();

        assert_eq!(serialised, RawMessage::new(1, vec![]).unwrap());

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_key_rejected() {
        let message = ShellServerMessage::KeyRejected;
        let serialised = message.serialise().unwrap();

        assert_eq!(serialised, RawMessage::new(2, vec![]).unwrap());

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_serialise_shell_started() {
        let message = ShellServerMessage::ShellStarted(ShellStartedPayload::LocalPty);
        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised,
            RawMessage::new(
                3,
                serde_json::to_vec(&ShellStartedPayload::LocalPty).unwrap()
            )
            .unwrap()
        );

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_serialise_stdout() {
        let message = ShellServerMessage::Stdout(vec![1, 2, 3, 4, 5]);
        let serialised = message.serialise().unwrap();

        assert_eq!(serialised, RawMessage::new(4, vec![1, 2, 3, 4, 5]).unwrap());

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_serialise_exited() {
        let message = ShellServerMessage::Exited(5);
        let serialised = message.serialise().unwrap();

        assert_eq!(serialised, RawMessage::new(5, vec![5]).unwrap());

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_server_serialise_error() {
        let message = ShellServerMessage::Error("test".to_owned());
        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised,
            RawMessage::new(255, "test".as_bytes().to_vec()).unwrap()
        );

        let deserialised = ShellServerMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }

    #[test]
    fn test_client_serialise_network() {
        let message = ShellClientMessage::Network(NetworkMessage::TcpSend(42, vec![1, 2, 3]));
        let serialised = message.serialise().unwrap();
        let json_len = serde_json::to_vec(&NetworkMessage::TcpSend(42, vec![1, 2, 3]))
            .unwrap()
            .len();

        assert_eq!(serialised.type_id(), 6);
        assert_eq!(serialised.data(), &vec![5, 0, 0, 0, 42, 1, 2, 3]);
        assert!(serialised.data().len() < json_len);

        let deserialised = ShellClientMessage::deserialise(&serialised).unwrap();

        assert_eq!(message, deserialised);
    }
}
