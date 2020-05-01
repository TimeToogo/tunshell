import {
  TlsRelayServerMessage,
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  TlsRelayClientMessage,
} from '../src/model';
import { TlsRelayMessageSerialiser } from '../src/serialisation';

describe('TlsRelayMessageSerialiser', () => {
  it('Serialises server messages', () => {
    const message: TlsRelayServerMessage = {
      type: TlsRelayServerMessageType.TIME_PLEASE,
      length: 5,
      data: Buffer.from('abcde'),
    };

    const buffer = new TlsRelayMessageSerialiser().serialise(message);

    expect(buffer.length).toBe(8);
    expect(buffer[0]).toBe(TlsRelayServerMessageType.TIME_PLEASE);
    expect(buffer[1]).toBe(0);
    expect(buffer[2]).toBe(5);
    expect(buffer.slice(3).toString()).toBe('abcde');
  });

  it('Serialises client messages', () => {
    const message: TlsRelayClientMessage = {
      type: TlsRelayClientMessageType.KEY,
      length: 10,
      data: Buffer.from('1234567890'),
    };

    const buffer = new TlsRelayMessageSerialiser().serialise(message);

    expect(buffer.length).toBe(13);
    expect(buffer[0]).toBe(TlsRelayClientMessageType.KEY);
    expect(buffer[1]).toBe(0);
    expect(buffer[2]).toBe(10);
    expect(buffer.slice(3).toString()).toBe('1234567890');
  });

  it('Serialises message without buffer', () => {
    const message: TlsRelayClientMessage = {
      type: TlsRelayClientMessageType.CLOSE,
      length: 0,
    };

    const buffer = new TlsRelayMessageSerialiser().serialise(message);

    expect(buffer.length).toBe(3);
    expect(buffer[0]).toBe(TlsRelayClientMessageType.CLOSE);
    expect(buffer[1]).toBe(0);
    expect(buffer[2]).toBe(0);
  });

  it('Throws error when serialising message with length and no buffer', () => {
    const message: TlsRelayClientMessage = {
      type: TlsRelayClientMessageType.CLOSE,
      length: 5,
    };

    expect(() =>
      new TlsRelayMessageSerialiser().serialise(message),
    ).toThrowError();
  });

  it('Deserialises server messages', () => {
    const buffer = Buffer.alloc(10);
    buffer[0] = TlsRelayServerMessageType.KEY_REJECTED;
    buffer[1] = 0;
    buffer[2] = 7;
    buffer.fill('1'.charCodeAt(0), 3);

    const message = new TlsRelayMessageSerialiser().deserialise<
      TlsRelayClientMessage
    >(buffer);

    expect(message.type).toBe(TlsRelayServerMessageType.KEY_REJECTED);
    expect(message.length).toBe(7);
    expect(message.data.toString()).toBe('1111111');
  });

  it('Deserialises client messages', () => {
    const buffer = Buffer.alloc(8);
    buffer[0] = TlsRelayClientMessageType.KEY;
    buffer[1] = 0;
    buffer[2] = 5;
    buffer.fill('1'.charCodeAt(0), 3);

    const message = new TlsRelayMessageSerialiser().deserialise<
      TlsRelayClientMessage
    >(buffer);

    expect(message.type).toBe(TlsRelayClientMessageType.KEY);
    expect(message.length).toBe(5);
    expect(message.data.toString()).toBe('11111');
  });

  it('Throw error when deserialising buffer that is too small', () => {
    expect(() =>
      new TlsRelayMessageSerialiser().deserialise(Buffer.alloc(1)),
    ).toThrowError();
  });

  it('Returns the same message after serialise then deserialise', () => {
    const message: TlsRelayClientMessage = {
      type: TlsRelayClientMessageType.KEY,
      length: 10,
      data: Buffer.from('1234567890'),
    };

    const serialiser = new TlsRelayMessageSerialiser();
    const result = serialiser.deserialise(serialiser.serialise(message));

    expect(result).toStrictEqual(message);
  });

});
