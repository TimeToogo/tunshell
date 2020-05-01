export enum TlsRelayConnectionState {
  NEW,
  WAITING_FOR_KEY,
  EXPIRED_WAITING_FOR_KEY,
  KEY_INVALID,
  WAITING_FOR_PEER,
  PEER_ALREADY_ENGAGED,
  EXPIRED_WAITING_FOR_PEER,
  NEGOTIATING_CONNECTION,
  EXPIRED_NEGOTIATING_CONNECTION,
  DIRECT_CONNECTION,
  RELAYED_CONNECTION,
  BROKEN_RELAY,
}

export interface TlsRelayMessage<T = number> {
  type: T; // uint8
  length: number; // uint16
  data?: Buffer; // uint8[]
}

export enum TlsRelayServerMessageType {
  CLOSE = 0,
  KEY_ACCEPTED = 1,
  KEY_REJECTED = 2,
  PEER_JOINED = 3,
  PEER_LEFT = 4,
  TIME_PLEASE = 5,
  ATTEMPT_DIRECT_CONNECT = 6,
  START_RELAY_MODE = 7,
}

export enum TlsRelayClientMessageType {
  CLOSE = 0,
  KEY = 1,
  TIME = 2,
  DIRECT_CONNECT_SUCCEEDED = 4,
  DIRECT_CONNECT_FAILED = 5,
  RELAY = 6,
}

export interface TlsRelayServerMessage
  extends TlsRelayMessage<TlsRelayServerMessageType> {}

export interface TlsRelayClientMessage
  extends TlsRelayMessage<TlsRelayClientMessageType> {}
