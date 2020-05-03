import * as stream from 'stream';
import * as tls from 'tls';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessageType,
  TlsRelayServerMessageType,
} from '@timetoogo/debug-my-pipeline--shared';
import { formatWithOptions } from 'util';

export class RelaySocket extends stream.Duplex {
  private readonly serialiser = new TlsRelayMessageSerialiser();
  private bytesRead: number = 0;
  private bytesPushed: number = 0;

  constructor(private readonly relaySocket: tls.TLSSocket) {
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
    this.relaySocket.on('data', (data) => {
      const messages = this.serialiser.deserialiseStream(data);
      
      for (const message of messages) {
        if (message.type === TlsRelayServerMessageType.RELAY) {
          console.log('read', message.data);
          this.push(message.data);
          this.bytesPushed += message.data.length;
        }
      }
    });
  };

  _read = async (size: number) => {
    this.bytesRead += size;
    console.log('read', this.bytesPushed, this.bytesRead, this.relaySocket.readable);
    while (this.bytesPushed < this.bytesRead && this.relaySocket.readable) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  };

  _write = (data: Buffer, encoding: string, callback: (error?: Error | null) => void): void => {
    console.log('write', data);
    if (!(data instanceof Buffer)) {
      data = Buffer.from(data, encoding as any);
    }

    const message = this.serialiser.serialise({
      type: TlsRelayClientMessageType.RELAY,
      length: data.length,
      data,
    });

    this.relaySocket.write(message, callback);
  };

  _final = (callback: (error?: Error | null) => void): void => {
    this.relaySocket.end(callback);
  };
}
