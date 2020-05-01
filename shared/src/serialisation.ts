import {
  TlsRelayMessage,
  TlsRelayClientMessage,
  TlsRelayServerMessage,
} from './model';

export class TlsRelayMessageSerialiser {
  public serialise = (
    message: TlsRelayClientMessage | TlsRelayServerMessage,
  ): Buffer => {
    const length = 3 + message.length;
    const buffer = Buffer.alloc(length);
    buffer[0] = message.type;
    buffer[1] = (message.length >> 8) & 0xff;
    buffer[2] = message.length & 0xff;

    if (message.length !== 0) {
      if (!message.data) {
        throw new Error(`data is required for message with length > 0`);
      }

      message.data.copy(buffer, 3, 0, message.length);
    }

    return buffer;
  };

  public deserialise<T extends TlsRelayMessage>(buffer: Buffer): T {
    if (buffer.length < 3) {
      throw new Error(`Buffer too small`);
    }

    const type = buffer[0] as T['type'];
    const length = (buffer[1] << 8) | buffer[2];
    const data = buffer.slice(3, 3 + length);
    const message = {
      type,
      length,
      data,
    } as T;

    return message;
  }
}
