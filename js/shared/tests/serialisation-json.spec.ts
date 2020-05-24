import {
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  TlsRelayJsonMessage,
} from '../src/model';
import { TlsRelayMessageSerialiser } from '../src/serialisation';

describe('TlsRelayMessageSerialiser', () => {
  it('Serialises json messages', () => {
    const message: TlsRelayJsonMessage<{ test: string }> = {
      type: TlsRelayServerMessageType.TIME_PLEASE,
      data: { test: 'hello' },
    };

    const buffer = new TlsRelayMessageSerialiser().serialiseJson(message);

    expect(buffer.length).toBe(19);
    expect(buffer[0]).toBe(TlsRelayServerMessageType.TIME_PLEASE);
    expect(buffer[1]).toBe(0);
    expect(buffer[2]).toBe(16);
    expect(buffer.slice(3).toString()).toBe('{"test":"hello"}');
  });

  it('Deserialises buffer json messages', () => {
    const buffer = Buffer.alloc(18);
    buffer[0] = TlsRelayClientMessageType.TIME;
    buffer[1] = 0;
    buffer[2] = 15;
    Buffer.from('{"time": "foo"}').copy(buffer, 3);

    const message = new TlsRelayMessageSerialiser().deserialiseJson<{
      time: string;
    }>(buffer);

    expect(message.type).toBe(TlsRelayClientMessageType.TIME);
    expect(message.data.time).toBe('foo');
  });
});
