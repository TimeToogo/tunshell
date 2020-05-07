import { EventEmitter } from 'events';
import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session } from '../session/session.model';
import * as tls from 'tls';
import { ClassProvider, Inject, LoggerService, Logger } from '@nestjs/common';
import { TlsRelayConfig } from './relay.config';
import { LatencyEstimation } from './relay.model';
import { TlsRelayConnection } from './relay.connection';
import * as _ from 'lodash';

export class TlsRelayServer {
  private server: tls.Server | null = null;
  private cleanUpIntervalId: NodeJS.Timeout;
  private readonly connections = {} as { [key: string]: TlsRelayConnection };
  private listenResolve: Function;
  private isUpdatingSessions: boolean = false;
  private sessionsToUpdate: (Session & Document)[] = [];

  constructor(
    @Inject('TlsConfig') private readonly config: TlsRelayConfig,
    @Inject('Logger') private readonly logger: LoggerService,
    @InjectModel('Session') private sessionModel: Model<Session & Document>,
  ) {}

  public getServer = () => this.server;
  public getConnections = () => this.connections;

  public listen = (port: number): Promise<void> => {
    this.server = tls.createServer(this.config.server, this.handleConnection);

    this.server.listen(port, () => {
      this.logger.log(`TLS Relay server listening on ${port}`);
      this.cleanUpIntervalId = setInterval(this.cleanUpOldConnections, this.config.cleanUpInterval);
    });

    return new Promise(resolve => (this.listenResolve = resolve));
  };

  public close = async () => {
    if (!this.server) {
      throw new Error(`Server is not listening`);
    }

    this.logger.log(`Tearing down TLS Relay server`);
    clearInterval(this.cleanUpIntervalId);

    for (const connection of _.values(this.connections)) {
      connection.close();
    }

    await new Promise((resolve, reject) => this.server.close(err => (err ? reject(err) : resolve())));
    this.server = null;
    this.listenResolve();
    this.listenResolve = null;
  };

  private handleConnection = (socket: tls.TLSSocket) => {
    this.logger.log(`Received connection from ${socket.remoteAddress}:${socket.remotePort}`);

    const connection = new TlsRelayConnection(this.config.connection, socket, this.logger);

    connection.on('key-received', () => {
      this.handleConnectionKey(connection).catch(this.logger.error);
    });

    connection.on('session-updated', () => {
      this.handleSessionUpdated(connection);
    });

    connection.waitForKey();
  };

  private handleConnectionKey = async (connection: TlsRelayConnection) => {
    try {
      const key = connection.getKey();

      let session: Session & Document = await this.sessionModel.findOne({
        $or: [{ 'host.key': key }, { 'client.key': key }],
      });

      if (!session || !this.isSessionValidToJoin(session, key)) {
        await connection.rejectKey();
        return;
      }

      const peer = [session.client, session.host].find(i => i.key !== key);
      const peerConnection =
        this.connections[peer.key] && !this.connections[peer.key].isDead() ? this.connections[peer.key] : null;

      // Ensure that peer share the same session instance
      if (peerConnection) {
        session = peerConnection.getSession();
      }

      this.connections[key] = connection;
      await connection.acceptKey(session);

      if (peerConnection) {
        await this.negotiateConnection(connection, peerConnection);
      }
    } catch (e) {
      this.logger.error(`Error occurred during TLS Relay connection: ${e.message}`, e.stack);
    }
  };

  private handleSessionUpdated = (connection: TlsRelayConnection) => {
    const session = connection.getSession();

    if (!session) {
      return;
    }

    if (this.sessionsToUpdate.includes(session)) {
      return;
    }

    this.sessionsToUpdate.push(session);
    this.updateSessions();
  };

  private updateSessions =  async () => {
    if (this.isUpdatingSessions) {
      return;
    }
    this.isUpdatingSessions = true;

    try {
      let session: Session&Document;

      while(session = this.sessionsToUpdate.shift()) {
        await session.save();
      }
    } finally {
      this.isUpdatingSessions = false;
    }
  }

  private negotiateConnection = async (peer1: TlsRelayConnection, peer2: TlsRelayConnection) => {
    await Promise.all([peer1.peerJoined(peer2), peer2.peerJoined(peer1)]);

    if (await this.negotiateDirectConnection(peer1, peer2)) {
      this.logger.log(`Successfully negotiated direct connection between peers`);
      return;
    }

    await this.configureRelayedConnection(peer1, peer2);
  };

  private negotiateDirectConnection = async (
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ): Promise<boolean> => {
    // All in milliseconds
    let estimate1: LatencyEstimation | undefined;
    let estimate2: LatencyEstimation | undefined;

    try {
      [estimate1, estimate2] = await Promise.all([peer1.estimateLatency(), peer2.estimateLatency()]);
    } catch (e) {
      this.logger.error(`Error while estimating peer latencies: ${e.message}`, e.stack);
      return false;
    }

    const maxLatencyMs = Math.max(estimate1.sendLatency, estimate2.sendLatency);
    const bufferMs = 500;
    const directConnectAttemptTime = Date.now() + maxLatencyMs + bufferMs;

    const directConnectTime1 = directConnectAttemptTime + estimate1.timeDiff;
    const directConnectTime2 = directConnectAttemptTime + estimate2.timeDiff;

    const [success1, success2] = await Promise.all([
      peer1.attemptDirectConnect(directConnectTime1),
      peer2.attemptDirectConnect(directConnectTime2),
    ]);

    return success1 && success2;
  };

  private configureRelayedConnection = async (peer1: TlsRelayConnection, peer2: TlsRelayConnection) => {
    this.logger.log(`Falling back to relay connection between peers`);
    await Promise.all([peer1.enableRelay(), peer2.enableRelay()]);
  };

  private isSessionValidToJoin = (session: Session, key: string): boolean => {
    // Check session has not expired
    if (Date.now() - session.createdAt.getTime() > this.config.keyExpiryTime) {
      return false;
    }

    const participant = [session.client, session.host].find(i => i.key === key);

    if (!participant) {
      return false;
    }

    // Cannot join twice
    return !participant.joined;
  };

  private cleanUpOldConnections = () => {
    let i = 0;

    for (const key in this.connections) {
      const connection = this.connections[key];

      if (connection.isDead()) {
        delete this.connections[key];
        i++;
      }
    }

    if (i !== 0) {
      this.logger.log(`TLS Relay cleaned up ${i} old connections`);
    }
  };
}

export const tlsRelayServerProvider: ClassProvider<TlsRelayServer> = {
  provide: 'TlsRelayServer',
  useClass: TlsRelayServer,
};
