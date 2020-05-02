import { TlsRelayMessage, TlsRelayJsonMessage } from './model';

export class TlsRelayMessageSerialiser {
  public serialise = (message: TlsRelayMessage): Buffer => {
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

  public serialiseJson = <T extends TlsRelayJsonMessage<{}>>(
    message: T,
  ): Buffer => {
    const json = Buffer.from(JSON.stringify(message.data), 'utf8');

    return this.serialise({
      type: message.type,
      length: json.length,
      data: json,
    });
  };

  public deserialise = <T extends TlsRelayMessage>(buffer: Buffer): T => {
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
  };

  public deserialiseJson = <TData>(
    buffer: Buffer | TlsRelayMessage,
  ): TlsRelayJsonMessage<Partial<TData>> => {
    const message =
      buffer instanceof Buffer ? this.deserialise(buffer) : buffer;

    let parsedData: Partial<TData> | null = null;

    try {
      parsedData = JSON.parse(message.data.toString('utf8'));
    } catch (e) {
      throw new Error(`Failed to parse JSON from message: ${e.message}`);
    }

    return {
      type: message.type,
      data: parsedData!,
    } as TlsRelayJsonMessage<Partial<TData>>;
  };
}
