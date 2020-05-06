import { Stream } from 'stream';
import { Socket } from 'net';
import { TlsRelayMessageSerialiser } from './serialisation';
import { TlsRelayMessage, TlsRelayJsonMessage } from './model';

export class TlsRelayMessageDuplexStream<TInput extends {}, TOutput extends {}> extends Stream.Duplex {
  private readonly serialiser = new TlsRelayMessageSerialiser();
  private messageBuffer: Buffer | null = null;

  constructor(
    private readonly innerStream: Socket,
    private readonly inputTypes: TInput,
    private readonly outputTypes: TOutput,
  ) {
    super({ objectMode: true });

    this.forwardEvents();
    this.init();
  }

  private forwardEvents = () => {
    this.innerStream.on('close', (err) => this.emit('close', err));
    this.innerStream.on('connect', () => this.emit('connect'));
    this.innerStream.on('drain', () => this.emit('drain'));
    this.innerStream.on('end', () => this.emit('end'));
    this.innerStream.on('error', (err) => this.emit('error', err));
    this.innerStream.on('ready', () => this.emit('ready'));
    this.innerStream.on('timeout', () => this.emit('timeout'));
  };

  private init = () => {
    this.innerStream.on('data', this.handleData);
  };

  private handleData = (data: Buffer) => {
    if (this.messageBuffer) {
      this.messageBuffer = Buffer.concat([Uint8Array.from(this.messageBuffer), Uint8Array.from(data)]);
    } else {
      this.messageBuffer = data;
    }

    const headerLength = 3;

    while (this.messageBuffer.length >= headerLength) {
      const messageType = this.messageBuffer[0];

      if (!this.outputTypes[messageType]) {
        this.emit(
          'error',
          new Error(
            `Error while reading message stream, expecting valid message type at offset but received: ${messageType}`,
          ),
        );
        return;
      }

      const messageLength = (this.messageBuffer[1] << 8) | this.messageBuffer[2];

      if (this.messageBuffer.length < messageLength + headerLength) {
        break;
      }

      const message: TlsRelayMessage = {
        type: messageType,
        length: messageLength,
        data: this.messageBuffer.slice(headerLength, messageLength + headerLength),
      };

      this.push(message);

      this.messageBuffer = this.messageBuffer.slice(headerLength + messageLength);
    }
  };

  _read = (size: number) => {};

  _write = (message: TlsRelayMessage, encoding, callback) => {
    if (!this.inputTypes[message.type]) {
      throw new Error(`Cannot write message with type ${message.type} to stream.`);
    }

    if (!this.innerStream.writable) {
      throw new Error(`Inner stream is not writable`)
    }

    this.innerStream.write(this.serialiser.serialise(message), callback);
  };

  writeJson = <T>(message: TlsRelayJsonMessage<T>, encoding?, callback?) => {
    const data = Buffer.from(JSON.stringify(message.data), 'utf8');

    const rawMessage = {
      type: message.type,
      length: data.length,
      data,
    };

    this.write(rawMessage, 'utf8', callback);
  };

  _final = (cb?: Function) => {
    this.innerStream.end(cb as any);
  };
}
