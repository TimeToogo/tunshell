import * as fs from 'fs';
import * as tls from 'tls';
import * as _ from 'lodash';
import { Logger } from '@nestjs/common';
import { Model, Document } from 'mongoose';
import { TlsRelayConfig } from '../src/tls/tls.config';
import { TlsRelayServer } from '../src/tls/tls.server';
import { Session } from '../src/session/session.model';
import { TlsRelayConnection } from '../src/tls/tls.connection';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessageType,
} from '@timetoogo/debug-my-pipeline--shared';

const mockConfig = (config: Partial<TlsRelayConfig> = {}): TlsRelayConfig =>
  _.defaultsDeep(
    {
      server: {
        key: fs.readFileSync(__dirname + '/../certs/development.key'),
        cert: fs.readFileSync(__dirname + '/../certs/development.cert'),
      },

      cleanUpInterval: 60 * 1000,
      negotiateConnectionTimeout: 10 * 1000,

      connection: {
        waitForKeyTimeout: 5000,
        waitForPeerTimeout: 600 * 1000,
        connectionTimeLimit: 3600 * 1000,
      },
    },
    config,
  );

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

describe('TlsServer', () => {
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
        ipAddress: '123.123.123.123',
        joined: false,
      },
      client: {
        key: 'client-key--1234567890',
        ipAddress: '123.123.123.123',
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
    } finally {
      await server.close();
      await closeSocket(socket);
    }
  });
});
