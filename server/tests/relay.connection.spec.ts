import * as fs from 'fs';
import * as tls from 'tls';
import * as _ from 'lodash';
import { EventEmitter } from 'events';
import { Logger } from '@nestjs/common';
import { TlsRelayConfig } from '../src/relay/relay.config';
import { Session } from '../src/session/session.model';
import { TlsRelayConnection } from '../src/relay/relay.connection';
import { TlsRelayConnectionState } from '../src/relay/relay.model';
import {
  TlsRelayMessageSerialiser,
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  ClientTimePayload,
} from '@timetoogo/debug-my-pipeline--shared';

const mockConfig = (config: Partial<TlsRelayConfig['connection']> = {}): TlsRelayConfig['connection'] =>
  _.defaultsDeep(config, {
    waitForKeyTimeout: 5 * 1000,
    waitForPeerTimeout: 600 * 1000,
    estimateLatencyTimeout: 5 * 1000,
    attemptDirectConnectTimeout: 10 * 1000,
    connectionTimeLimit: 3600 * 1000,
  });

const mockSocket = (
  config: Partial<TlsRelayConfig['connection']> = {},
): tls.TLSSocket & {
  write: jest.MockedFunction<tls.TLSSocket['write']>;
  on: jest.MockedFunction<EventEmitter['on']>;
  end: jest.MockedFunction<tls.TLSSocket['end']>;
} => {
  const mock = new EventEmitter() as any;

  mock.writable = true;
  mock.write = jest.fn(async (data, cb) => {
    await new Promise(resolve => setTimeout(resolve, 1));
    cb();
  });
  mock.end = jest.fn();
  mock.destroy = jest.fn();

  return mock;
};

const logger = new Logger('TlsRelayConnectionTests');
const serialiser = new TlsRelayMessageSerialiser();

describe('TlsRelayConnection', () => {
  it('Has correct initial state', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      expect(connection.getKey()).toBe(null);
      expect(connection.isDead()).toBe(false);
      expect(connection.getParticipant()).toBe(null);
      expect(connection.getSession()).toBe(null);
      expect(connection.getState()).toBe(TlsRelayConnectionState.NEW);
    } finally {
      connection.close();
    }
  });

  it('Close method closes the connection', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      await connection.close();

      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
        data: Buffer.alloc(0),
      });

      expect(socket.end.mock.calls.length).toBe(1);
      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(connection.isDead()).toBe(true);
    } finally {
      connection.close();
    }
  });

  it('Closes when receives invalid message', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      socket.emit('data', Buffer.from('invalid-message'));

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
        data: Buffer.alloc(0),
      });
    } finally {
      connection.close();
    }
  });

  it('Receiving key triggers key-received event handler', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      connection.waitForKey();

      let eventTriggered = false;

      connection.on('key-received', () => (eventTriggered = true));

      socket.emit(
        'data',
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: 20,
          data: Buffer.from('12345678900987654321'),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 1));

      expect(connection.getKey()).toBe('12345678900987654321');
      expect(eventTriggered).toBe(true);
    } finally {
      connection.close();
    }
  });

  it('Rejecting key closes the connection', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      connection.waitForKey();
      await connection.rejectKey();

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.KEY_REJECTED,
        length: 0,
        data: Buffer.alloc(0),
      });
      expect(serialiser.deserialise(socket.write.mock.calls[1][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
        data: Buffer.alloc(0),
      });
    } finally {
      connection.close();
    }
  });

  it('Enters EXPIRED_WAITING_FOR_KEY state after configured timeout', async () => {
    const config = mockConfig({ waitForKeyTimeout: 50 });
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      connection.waitForKey();

      await new Promise(resolve => setTimeout(resolve, 110));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
        data: Buffer.alloc(0),
      });
      expect(connection.isDead()).toBe(true);
    } finally {
      connection.close();
    }
  });

  it('Accepting key enters WAITING_FOR_PEER state', async () => {
    const config = mockConfig();
    const socket = mockSocket();
    const mockSession = { host: {}, client: {}, save: jest.fn() } as any;

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      connection.waitForKey();

      await connection.acceptKey(mockSession);

      expect(connection.getState()).toBe(TlsRelayConnectionState.WAITING_FOR_PEER);
      expect(connection.getSession()).toBe(mockSession);
      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.KEY_ACCEPTED,
        length: 0,
        data: Buffer.alloc(0),
      });
    } finally {
      connection.close();
    }
  });

  it('Notifies client that peer has joined', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);
    const peer = ({
      getKey: () => 'peer-key',
      getSocket: () => ({ remoteAddress: 'peer-ip' }),
    } as any) as TlsRelayConnection;

    try {
      (connection as any).state = TlsRelayConnectionState.WAITING_FOR_PEER;
      await connection.peerJoined(peer);

      expect(serialiser.deserialiseJson(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.PEER_JOINED,
        data: { peerKey: 'peer-key', peerIpAddress: 'peer-ip' },
      });
    } finally {
      connection.close();
    }
  });

  it('Estimates latency accurately', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);
    const responseDelay = 100;

    try {
      setTimeout(() => {
        socket.emit(
          'data',
          serialiser.serialiseJson<ClientTimePayload>({
            type: TlsRelayClientMessageType.TIME,
            data: {
              clientTime: Date.now() - 50,
            },
          }),
        );
      }, responseDelay);

      const estimation = await connection.estimateLatency();

      expect(estimation.sendLatency).toBeCloseTo(50, -1);
      expect(estimation.receiveLatency).toBeCloseTo(50, -1);
      expect(estimation.timeDiff).toBeCloseTo(0, -1);

      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.TIME_PLEASE,
        length: 0,
        data: Buffer.alloc(0),
      });
    } finally {
      connection.close();
    }
  });

  it('Times out when receiving no response to TIME_PLEASE', async () => {
    const config = mockConfig({ estimateLatencyTimeout: 50 });
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      await expect(connection.estimateLatency()).rejects.toThrowError();

      await new Promise(resolve => setTimeout(resolve, 1));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      connection.close();
    }
  });

  it('Behaves correctly for a successful direct connection', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      const coordinatedTime = Date.now();

      setTimeout(
        () =>
          socket.emit(
            'data',
            serialiser.serialise({
              type: TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED,
              length: 0,
            }),
          ),
        10,
      );

      const result = await connection.attemptDirectConnect(coordinatedTime);

      expect(result).toBe(true);
      expect(serialiser.deserialiseJson(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
        data: {
          connectAt: coordinatedTime,
        },
      });
      expect(connection.getState()).toBe(TlsRelayConnectionState.DIRECT_CONNECTION);
    } finally {
      connection.close();
    }
  });

  it('Behaves correctly for a failed direct connection', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      const coordinatedTime = Date.now();

      setTimeout(
        () =>
          socket.emit(
            'data',
            serialiser.serialise({
              type: TlsRelayClientMessageType.DIRECT_CONNECT_FAILED,
              length: 0,
            }),
          ),
        10,
      );

      const result = await connection.attemptDirectConnect(coordinatedTime);

      expect(result).toBe(false);
      expect(serialiser.deserialiseJson(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
        data: {
          connectAt: coordinatedTime,
        },
      });
      expect(connection.getState()).toBe(TlsRelayConnectionState.DIRECT_CONNECT_FAILED);
    } finally {
      connection.close();
    }
  });

  it('Behaves correctly for a timed out direct connection', async () => {
    const config = mockConfig({ attemptDirectConnectTimeout: 50 });
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      const coordinatedTime = Date.now();

      await expect(() => connection.attemptDirectConnect(coordinatedTime)).rejects.toThrowError();

      await new Promise(resolve => setTimeout(resolve, 1));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      connection.close();
    }
  });

  it('Direct connection time out closes connection', async () => {
    const config = mockConfig({ connectionTimeLimit: 50 });
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      const coordinatedTime = Date.now();

      setTimeout(
        () =>
          socket.emit(
            'data',
            serialiser.serialise({
              type: TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED,
              length: 0,
            }),
          ),
        10,
      );

      const result = await connection.attemptDirectConnect(coordinatedTime);

      expect(result).toBe(true);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(serialiser.deserialiseJson(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
        data: {
          connectAt: coordinatedTime,
        },
      });
      expect(serialiser.deserialise(socket.write.mock.calls[1][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
        data: Buffer.alloc(0),
      });
      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      connection.close();
    }
  });

  it('Enabling relayed connection operates correctly', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      await connection.enableRelay();

      expect(serialiser.deserialise(socket.write.mock.calls[0][0] as Buffer)).toStrictEqual({
        type: TlsRelayServerMessageType.START_RELAY_MODE,
        length: 0,
        data: Buffer.alloc(0),
      });
      expect(connection.getState()).toBe(TlsRelayConnectionState.RELAYED_CONNECTION);
    } finally {
      connection.close();
    }
  });

  it('Connection closes after relayed connection times out', async () => {
    const config = mockConfig({ connectionTimeLimit: 50 });
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);

    try {
      await connection.enableRelay();

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      connection.close();
    }
  });

  it('Relayed connection relayed data to peer', async () => {
    const config = mockConfig();
    const socket = mockSocket();

    const connection = new TlsRelayConnection(config, socket, logger);
    const peerConnection = {
      sendMessage: jest.fn(),
    };

    try {
      (connection as any).peer = peerConnection;
      await connection.enableRelay();

      expect(connection.getState()).toBe(TlsRelayConnectionState.RELAYED_CONNECTION);

      connection.getSocket().emit(
        'data',
        serialiser.serialise({
          type: TlsRelayClientMessageType.RELAY,
          length: 10,
          data: Buffer.from('hello peer'),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 10));

      expect(peerConnection.sendMessage.mock.calls[0][0]).toStrictEqual({
        type: TlsRelayServerMessageType.RELAY,
        length: 10,
        data: Buffer.from('hello peer'),
      });
    } finally {
      connection.close();
    }
  });
});
