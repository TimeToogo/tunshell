import * as fs from 'fs';
import * as tls from 'tls';
import * as _ from 'lodash';
import { Logger } from '@nestjs/common';
import { Model, Document } from 'mongoose';
import { TlsRelayConfig } from '../src/relay/relay.config';
import { TlsRelayServer } from '../src/relay/relay.server';
import { Session } from '../src/session/session.model';
import { TlsRelayConnection } from '../src/relay/relay.connection';
import { TlsRelayConnectionState } from '../src/relay/relay.model';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessageType,
  TlsRelayServerMessageType,
  ClientTimePayload,
} from '@timetoogo/debug-my-pipeline--shared';

const mockConfig = (config: Partial<TlsRelayConfig> = {}): TlsRelayConfig =>
  _.defaultsDeep(config, {
    server: {
      key: fs.readFileSync(__dirname + '/../certs/development.key'),
      cert: fs.readFileSync(__dirname + '/../certs/development.cert'),
    },

    cleanUpInterval: 60 * 1000,

    connection: {
      waitForKeyTimeout: 5 * 1000,
      waitForPeerTimeout: 600 * 1000,
      estimateLatencyTimeout: 5 * 1000,
      attemptDirectConnectTimeout: 10 * 1000,
      connectionTimeLimit: 3600 * 1000,
    },
  });

const logger = new Logger('TlsRelayServerTests');

const mockSessionRepo = (session?: Session): Model<Session & Document> =>
  ({
    findOne: () => session,
  } as any);

const connectToServer = (port: number): Promise<tls.TLSSocket> => {
  const socket = tls.connect({
    host: '127.0.0.1',
    port,
    ca: [mockConfig().server.cert as Buffer],
    checkServerIdentity: () => null,
  });
  return new Promise<tls.TLSSocket>((resolve, reject) => {
    socket.on('secureConnect', () => resolve(socket));
    socket.on('error', e => {
      closeSocket(socket).then(() => reject(e));
    });
  });
};

const closeSocket = async (socket: tls.TLSSocket): Promise<void> => {
  if (socket.destroyed) {
    return;
  }

  await new Promise((resolve, reject) => socket.end(resolve));
  socket.destroy();
};

let port = 60000;
const getPort = () => port++;

describe('TlsRelayServer', () => {
  it('Listens to connections on port', async () => {
    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo();

    const server = new TlsRelayServer(config, logger, repo);
    let closed = false;

    try {
      await expect(connectToServer(port)).rejects.toThrow();

      server.listen(port).then(() => (closed = true));

      expect(closed).toBe(false);

      const socket = await connectToServer(port);
      closeSocket(socket);
    } finally {
      await server.close();
      await new Promise(resolve => setTimeout(resolve, 1));
      expect(closed).toBe(true);
    }
  });

  it('Closes connection on invalid input', async () => {
    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo();

    const server = new TlsRelayServer(config, logger, repo);
    server.listen(port);
    const socket = await connectToServer(port);

    try {
      let ended = false;
      socket.write('some-invalid-input');

      socket.on('data', console.log);
      socket.on('end', () => (ended = true));

      await new Promise(resolve => setTimeout(resolve, 100));
      expect(ended).toBe(true);
    } finally {
      await server.close();
      await closeSocket(socket);
    }
  });

  it('Rejects invalid key', async () => {
    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo();
    const serialiser = new TlsRelayMessageSerialiser();

    const server = new TlsRelayServer(config, logger, repo);
    server.listen(port);
    const socket = await connectToServer(port);

    try {
      let ended = false;

      socket.on('data', console.log);
      socket.on('end', () => (ended = true));

      socket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: 16,
          data: Buffer.from('1234567890987654'),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 100));
      expect(ended).toBe(true);
    } finally {
      await server.close();
      await closeSocket(socket);
    }
  });

  it('Accepts valid key', async () => {
    const CURRENT_SESSION: Session & Partial<Document> = {
      host: {
        key: 'host-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      client: {
        key: 'client-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      createdAt: new Date(),
      save: (async () => {}) as any,
    };

    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo(CURRENT_SESSION);
    const serialiser = new TlsRelayMessageSerialiser();

    const server = new TlsRelayServer(config, logger, repo);
    server.listen(port);
    const socket = await connectToServer(port);

    try {
      let ended = false;

      socket.on('data', console.log);
      socket.on('end', () => (ended = true));

      socket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: CURRENT_SESSION.client.key.length,
          data: Buffer.from(CURRENT_SESSION.client.key),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 100));
      expect(ended).toBe(false);

      const connection = server.getConnections()[CURRENT_SESSION.client.key];
      expect(connection).toBeInstanceOf(TlsRelayConnection);
      expect(connection.getSession()).toBe(CURRENT_SESSION);
      expect(connection.getSession().host.joined).toBe(false);
      expect(connection.getSession().client.joined).toBe(true);
      expect(connection.getSession().client.ipAddress).toBe(connection.getSocket().remoteAddress);
    } finally {
      await server.close();
      await closeSocket(socket);
    }
  });

  it('Operates correctly during a direct connection', async () => {
    const CURRENT_SESSION: Session & Partial<Document> = {
      host: {
        key: 'host-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      client: {
        key: 'client-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      createdAt: new Date(),
      save: (async () => {}) as any,
    };

    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo(CURRENT_SESSION);
    const serialiser = new TlsRelayMessageSerialiser();

    const server = new TlsRelayServer(config, logger, repo);
    server.listen(port);

    const socket = await connectToServer(port);
    const peerSocket = await connectToServer(port);

    const socketLog = [];
    const peerLog = [];

    const handleServerRequests = (socket: tls.TLSSocket, log: any[][], data: Buffer) => {
      const message = serialiser.deserialise(data);

      switch (message.type) {
        case TlsRelayServerMessageType.KEY_ACCEPTED:
          log.push(['Key accepted!']);
          break;
        case TlsRelayServerMessageType.PEER_JOINED:
          log.push(['Peer joined', JSON.parse(message.data.toString())]);
          break;
        case TlsRelayServerMessageType.TIME_PLEASE:
          log.push(['Time please']);
          socket.write(
            serialiser.serialiseJson<ClientTimePayload>({
              type: TlsRelayClientMessageType.TIME,
              data: { clientTime: Date.now() },
            }),
          );
          break;
        case TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT:
          log.push(['Attempt direct connect']);
          socket.write(
            serialiser.serialise({
              type: TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED,
              length: 0,
            }),
          );
          break;
        default:
          console.error(log);
          console.error(message);
          throw new Error(`Unexpected message type: ${TlsRelayServerMessageType[message.type]}`);
      }
    };

    try {
      const socketHandler = data => handleServerRequests(socket, socketLog, data);
      const peerSocketHandler = data => handleServerRequests(peerSocket, peerLog, data);

      socket.on('data', socketHandler);
      peerSocket.on('data', peerSocketHandler);

      socket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: CURRENT_SESSION.client.key.length,
          data: Buffer.from(CURRENT_SESSION.client.key),
        }),
      );

      peerSocket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: CURRENT_SESSION.host.key.length,
          data: Buffer.from(CURRENT_SESSION.host.key),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 500));

      socket.off('data', socketHandler);
      peerSocket.off('data', peerSocketHandler);

      const connection = server.getConnections()[CURRENT_SESSION.client.key];
      const peerConnection = server.getConnections()[CURRENT_SESSION.host.key];

      expect(connection).toBeInstanceOf(TlsRelayConnection);
      expect(connection.getState()).toBe(TlsRelayConnectionState.DIRECT_CONNECTION);
      expect(peerConnection.getState()).toBe(TlsRelayConnectionState.DIRECT_CONNECTION);

      expect(connection.getSession()).toBe(peerConnection.getSession());

      expect(connection.getSession().host.joined).toBe(true);
      expect(connection.getSession().client.joined).toBe(true);

      expect(socketLog).toStrictEqual([
        ['Key accepted!'],
        [
          'Peer joined',
          {
            peerIpAddress: peerConnection.getSocket().remoteAddress,
            peerKey: 'host-key--1234567890',
          },
        ],
        ['Time please'],
        ['Attempt direct connect'],
      ]);
      expect(peerLog).toStrictEqual([
        ['Key accepted!'],
        [
          'Peer joined',
          {
            peerIpAddress: connection.getSocket().remoteAddress,
            peerKey: 'client-key--1234567890',
          },
        ],
        ['Time please'],
        ['Attempt direct connect'],
      ]);

      // At this point a direct P2P connection is assumed to be made
      // all data exchanged does not involve the relay server
      socket.end();

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(peerConnection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      await server.close();
      await closeSocket(socket);
      await closeSocket(peerSocket);
    }
  });

  it('Operates correctly during a relayed connection', async () => {
    const CURRENT_SESSION: Session & Partial<Document> = {
      host: {
        key: 'host-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      client: {
        key: 'client-key--1234567890',
        ipAddress: null,
        joined: false,
      },
      createdAt: new Date(),
      save: (async () => {}) as any,
    };

    const port = getPort();
    const config = mockConfig();
    const repo = mockSessionRepo(CURRENT_SESSION);
    const serialiser = new TlsRelayMessageSerialiser();

    const server = new TlsRelayServer(config, logger, repo);
    server.listen(port);

    const socket = await connectToServer(port);
    const peerSocket = await connectToServer(port);

    const socketLog = [];
    const peerLog = [];

    const handleServerRequests = (socket: tls.TLSSocket, log: any[][], data: Buffer) => {
      const message = serialiser.deserialise(data);

      switch (message.type) {
        case TlsRelayServerMessageType.KEY_ACCEPTED:
          log.push(['Key accepted!']);
          break;
        case TlsRelayServerMessageType.PEER_JOINED:
          log.push(['Peer joined', JSON.parse(message.data.toString())]);
          break;
        case TlsRelayServerMessageType.TIME_PLEASE:
          log.push(['Time please']);
          socket.write(
            serialiser.serialiseJson<ClientTimePayload>({
              type: TlsRelayClientMessageType.TIME,
              data: { clientTime: Date.now() },
            }),
          );
          break;
        case TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT:
          log.push(['Attempt direct connect']);
          socket.write(
            serialiser.serialise({
              type: TlsRelayClientMessageType.DIRECT_CONNECT_FAILED,
              length: 0,
            }),
          );
          break;
        case TlsRelayServerMessageType.START_RELAY_MODE:
          log.push(['Relay mode started']);
          break;
        case TlsRelayServerMessageType.RELAY:
          log.push(['Received relayed data', message.data.toString()]);
          break;
        default:
          console.error(log);
          console.error(message);
          throw new Error(`Unexpected message type: ${TlsRelayServerMessageType[message.type]}`);
      }
    };

    try {
      const socketHandler = data => handleServerRequests(socket, socketLog, data);
      const peerSocketHandler = data => handleServerRequests(peerSocket, peerLog, data);

      socket.on('data', socketHandler);
      peerSocket.on('data', peerSocketHandler);

      socket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: CURRENT_SESSION.client.key.length,
          data: Buffer.from(CURRENT_SESSION.client.key),
        }),
      );

      peerSocket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.KEY,
          length: CURRENT_SESSION.host.key.length,
          data: Buffer.from(CURRENT_SESSION.host.key),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 500));

      const connection = server.getConnections()[CURRENT_SESSION.client.key];
      const peerConnection = server.getConnections()[CURRENT_SESSION.host.key];

      expect(connection).toBeInstanceOf(TlsRelayConnection);
      expect(connection.getState()).toBe(TlsRelayConnectionState.RELAYED_CONNECTION);
      expect(peerConnection.getState()).toBe(TlsRelayConnectionState.RELAYED_CONNECTION);

      expect(connection.getSession()).toBe(peerConnection.getSession());

      expect(connection.getSession().host.joined).toBe(true);
      expect(connection.getSession().client.joined).toBe(true);

      // Test data relaying between the connections
      socket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.RELAY,
          length: 13,
          data: Buffer.from('hello to peer'),
        }),
      );

      peerSocket.write(
        serialiser.serialise({
          type: TlsRelayClientMessageType.RELAY,
          length: 15,
          data: Buffer.from('hello from peer'),
        }),
      );

      await new Promise(resolve => setTimeout(resolve, 100));

      socket.off('data', socketHandler);
      peerSocket.off('data', peerSocketHandler);

      expect(socketLog).toStrictEqual([
        ['Key accepted!'],
        [
          'Peer joined',
          {
            peerIpAddress: peerConnection.getSocket().remoteAddress,
            peerKey: 'host-key--1234567890',
          },
        ],
        ['Time please'],
        ['Attempt direct connect'],
        ['Relay mode started'],
        ['Received relayed data', 'hello from peer'],
      ]);
      expect(peerLog).toStrictEqual([
        ['Key accepted!'],
        [
          'Peer joined',
          {
            peerIpAddress: connection.getSocket().remoteAddress,
            peerKey: 'client-key--1234567890',
          },
        ],
        ['Time please'],
        ['Attempt direct connect'],
        ['Relay mode started'],
        ['Received relayed data', 'hello to peer'],
      ]);

      socket.end();

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(connection.getState()).toBe(TlsRelayConnectionState.CLOSED);
      expect(peerConnection.getState()).toBe(TlsRelayConnectionState.CLOSED);
    } finally {
      await server.close();
      await closeSocket(socket);
      await closeSocket(peerSocket);
    }
  });
});
