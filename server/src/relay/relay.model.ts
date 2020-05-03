export enum TlsRelayConnectionState {
  NEW,
  WAITING_FOR_KEY,
  EXPIRED_WAITING_FOR_KEY,
  KEY_INVALID,
  WAITING_FOR_PEER,
  EXPIRED_WAITING_FOR_PEER,
  NEGOTIATING_CONNECTION,
  NEGOTIATING__WAITING_FOR_TIME,
  EXPIRED_NEGOTIATING_CONNECTION,
  DIRECT_CONNECT_FAILED,
  DIRECT_CONNECTION,
  RELAYED_CONNECTION,
  EXPIRED_CONNECTION,
  CLOSED
}

export interface LatencyEstimation {
  // Server --> Client latency
  sendLatency: number
  // Client --> Server latency
  receiveLatency: number
  // Relative to server's time
  timeDiff: number
}