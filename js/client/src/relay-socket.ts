import * as stream from 'stream';
import * as tls from 'tls';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessageType,
  TlsRelayServerMessageType,
  TlsRelayMessageDuplexStream,
  TlsRelayClientMessage,
  TlsRelayServerMessage,
} from '@timetoogo/debug-my-pipeline--shared';
import { formatWithOptions } from 'util';

export class RelaySocket extends stream.Duplex {
  constructor(
    private readonly relaySocket: TlsRelayMessageDuplexStream<TlsRelayClientMessageType, TlsRelayServerMessageType>,
  ) {
    super();
    this.forwardEvents();
    this.captureRelayMessage();
  }

  private forwardEvents() {
    this.relaySocket.on('close', (err) => this.emit('close', err));
    this.relaySocket.on('connect', () => this.emit('connect'));
    this.relaySocket.on('drain', () => this.emit('drain'));
    this.relaySocket.on('end', () => this.emit('end'));
    this.relaySocket.on('error', (err) => this.emit('error', err));
    this.relaySocket.on('ready', () => this.emit('ready'));
    this.relaySocket.on('timeout', () => this.emit('timeout'));
  }

  private captureRelayMessage = () => {
    this.relaySocket.on('data', (message: TlsRelayServerMessage) => {
      if (message.type === TlsRelayServerMessageType.RELAY) {
        this.push(message.data);
      }
    });
  };

  _read = async (size: number) => {
  };

  _write = (data: Buffer, encoding: string, callback: (error?: Error | null) => void): void => {
    if (!(data instanceof Buffer)) {
      data = Buffer.from(data, encoding as any);
    }

    const message: TlsRelayClientMessage = {
      type: TlsRelayClientMessageType.RELAY,
      length: data.length,
      data,
    };

    if (!this.relaySocket.writable) {
      return;
    }

    this.relaySocket.write(message, callback);
  };

  _final = (callback: (error?: Error | null) => void): void => {
    this.relaySocket.end(callback);
  };
}
