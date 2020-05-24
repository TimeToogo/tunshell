import {
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  TlsRelayJsonMessage,
  TlsRelayMessage,
} from '../src/model';
import * as stream from 'stream';
import { TlsRelayMessageDuplexStream } from '../src/message-stream';

const collectMessages = async (stream: TlsRelayMessageDuplexStream<any, any>): Promise<TlsRelayMessage[]> => {
  const messages = [];

  stream.on('data', (message) => messages.push(message));

  return new Promise((resolve) => stream.on('end', () => resolve(messages)));
};

describe('TlsRelayMessageDuplexStream', () => {
  it('Reads single message from inner stream', async () => {
    const buffer = Buffer.alloc(13);
    buffer[0] = TlsRelayClientMessageType.KEY;
    buffer[1] = 0;
    buffer[2] = 10;
    buffer.fill('1', 3, 13);

    const messageStream = new TlsRelayMessageDuplexStream(
      stream.Readable.from([buffer]) as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    const messages = await collectMessages(messageStream);

    expect(messages).toStrictEqual([
      {
        type: TlsRelayClientMessageType.KEY,
        length: 10,
        data: Buffer.from('1111111111'),
      },
    ]);
  });

  it('Reads multiple consecutive messages', async () => {
    const buffer = Buffer.alloc(8);
    buffer[0] = TlsRelayClientMessageType.KEY;
    buffer[1] = 0;
    buffer[2] = 1;
    buffer[3] = 10;
    buffer[4] = TlsRelayClientMessageType.TIME;
    buffer[5] = 0;
    buffer[6] = 1;
    buffer[7] = 20;

    const messageStream = new TlsRelayMessageDuplexStream(
      stream.Readable.from([buffer]) as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    const messages = await collectMessages(messageStream);

    expect(messages).toStrictEqual([
      {
        type: TlsRelayClientMessageType.KEY,
        length: 1,
        data: Buffer.from([10]),
      },
      {
        type: TlsRelayClientMessageType.TIME,
        length: 1,
        data: Buffer.from([20]),
      },
    ]);
  });

  it('Reads multiple consecutive messages across packet boundaries', async () => {
    const buffer1 = Buffer.alloc(3);
    buffer1[0] = TlsRelayClientMessageType.KEY;
    buffer1[1] = 0;
    buffer1[2] = 1;
    const buffer2 = Buffer.alloc(3);
    buffer2[0] = 10;
    buffer2[1] = TlsRelayClientMessageType.TIME;
    buffer2[2] = 0;
    const buffer3 = Buffer.alloc(2);
    buffer3[0] = 1;
    buffer3[1] = 20;

    const messageStream = new TlsRelayMessageDuplexStream(
      stream.Readable.from([buffer1, buffer2, buffer3]) as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    const messages = await collectMessages(messageStream);

    expect(messages).toStrictEqual([
      {
        type: TlsRelayClientMessageType.KEY,
        length: 1,
        data: Buffer.from([10]),
      },
      {
        type: TlsRelayClientMessageType.TIME,
        length: 1,
        data: Buffer.from([20]),
      },
    ]);
  });

  it('Does not read partial message', async () => {
    const buffer = Buffer.alloc(4);
    buffer[0] = TlsRelayClientMessageType.KEY;
    buffer[1] = 0;
    buffer[2] = 2;
    buffer[3] = 10;

    const messageStream = new TlsRelayMessageDuplexStream(
      stream.Readable.from([buffer]) as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    const messages = await collectMessages(messageStream);

    expect(messages).toStrictEqual([]);
  });

  it('Throws error on unexpected message type', async () => {
    const buffer = Buffer.alloc(8);
    buffer[0] = 99;
    buffer[1] = 0;
    buffer[2] = 2;
    buffer[3] = 10;

    const messageStream = new TlsRelayMessageDuplexStream(
      stream.Readable.from([buffer]) as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    let error;

    messageStream.on('error', (err) => (error = err));

    await new Promise((resolve) => setImmediate(resolve));

    expect(error).toBeInstanceOf(Error);
  });

  it('Writes messages to underlying socket', async () => {
    const packets = [];
    let ended = false;

    const innerStream = new stream.Writable({
      write(data, encoding, callback) {
        packets.push(data);
        callback();
      },
      final(callback) {
        ended = true;
        callback();
      },
    });

    const messageStream = new TlsRelayMessageDuplexStream(
      innerStream as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    messageStream.write({
      type: TlsRelayClientMessageType.KEY,
      length: 1,
      data: Buffer.from([10]),
    });

    await new Promise((resolve) => messageStream.end(resolve));

    expect(packets).toStrictEqual([Buffer.from([TlsRelayClientMessageType.KEY, 0, 1, 10])]);

    expect(ended).toBe(true);
  });

  it('Writes JSON messages to underlying socket', async () => {
    const packets = [];
    let ended = false;

    const innerStream = new stream.Writable({
      write(data, encoding, callback) {
        packets.push(data);
        callback();
      },
      final(callback) {
        ended = true;
        callback();
      },
    });

    const messageStream = new TlsRelayMessageDuplexStream(
      innerStream as any,
      TlsRelayClientMessageType,
      TlsRelayServerMessageType,
    );

    messageStream.writeJson({
      type: TlsRelayClientMessageType.TIME,
      data: { foo: 'bar' },
    });

    await new Promise((resolve) => messageStream.end(resolve));

    expect(packets).toStrictEqual([
      Buffer.from([TlsRelayClientMessageType.TIME, 0, 13, ...Buffer.from('{"foo":"bar"}')]),
    ]);

    expect(ended).toBe(true);
  });
});
