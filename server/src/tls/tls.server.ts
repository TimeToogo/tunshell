import { EventEmitter } from 'events';
import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session } from '../session/session.model';
import * as tls from 'tls';
import { ClassProvider, Inject, LoggerService, Logger } from '@nestjs/common';
import { TlsRelayConfig } from './tls.config';
import { LatencyEstimation } from './tls.model';
import { TlsRelayConnection } from './tls.connection';
import * as _ from 'lodash';

export class TlsRelayServer {
  private server: tls.Server | null = null;
  private cleanUpIntervalId: NodeJS.Timeout;
  private readonly connections = {} as { [key: string]: TlsRelayConnection };
  private listenResolve: Function;

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
      this.cleanUpIntervalId = setInterval(
        this.cleanUpOldConnections,
        this.config.cleanUpInterval,
      );
    });

    return new Promise(resolve => (this.listenResolve = resolve));
  };

  public close = async () => {
    if (!this.server) {
      throw new Error(`Server is not listening`);
    }

    this.logger.log(`Tearing down TLS Relay server`);
    clearInterval(this.cleanUpIntervalId);

    for(const connection of _.values(this.connections)) {
      connection.close()
    }

    await new Promise((resolve, reject) =>
      this.server.close(err => (err ? reject(err) : resolve())),
    );
    this.server = null;
    this.listenResolve();
    this.listenResolve = null;
  };

  private handleConnection = (socket: tls.TLSSocket) => {
    this.logger.log(`Received connection from ${socket.remoteAddress}`);

    const connection = new TlsRelayConnection(
      this.config.connection,
      socket,
      this.logger,
    );

    connection.init();

    connection.on('key-received', () => {
      this.handleConnectionKey(connection);
    });
  };

  private handleConnectionKey = async (connection: TlsRelayConnection) => {
    const key = connection.getKey();

    let session: Session & Document = await this.sessionModel.findOne({
      $or: [{ 'host.key': key }, { 'client.key': key }],
    });

    if (!session || !this.isSessionValidToJoin(session, key)) {
      connection.rejectKey();
      return;
    }

    const peer = [session.client, session.host].find(i => i.key !== key);
    const peerConnection = this.connections[peer.key];

    // Ensure that peer share the same session instance
    if (peerConnection) {
      session = peerConnection.getSession();
    }

    this.connections[key] = connection;
    await connection.acceptKey(session);

    if (peerConnection) {
      this.negotiateConnection(connection, peerConnection);
    }
  };

  private negotiateConnection = async (
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ) => {
    peer1.peerJoined(peer2);
    peer2.peerJoined(peer1);

    if (await this.negotiateDirectConnection(peer1, peer2)) {
      return;
    }

    this.configureRelayedConnection(peer1, peer2);
  };

  private negotiateDirectConnection = (
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ): Promise<boolean> => {
    return new Promise<boolean>(async (resolve, reject) => {
      let cancelled: any = false;

      setTimeout(() => {
        cancelled = true;
        resolve(false);
      }, this.config.negotiateConnectionTimeout);

      // All in milliseconds
      let estimate1: LatencyEstimation | undefined;
      let estimate2: LatencyEstimation | undefined;

      try {
        [estimate1, estimate2] = await Promise.all([
          peer1.estimateLatency(),
          peer2.estimateLatency(),
        ]);
      } catch (e) {
        this.logger.log(
          `Error while estimating peer latencies: ${e.message}`,
          e.stack,
        );
        resolve(false);
      }

      if (cancelled === true) return;

      const maxLatencyMs = Math.max(
        estimate1.sendLatency,
        estimate2.sendLatency,
      );
      const bufferMs = 500;
      const directConnectAttemptTime = Date.now() + maxLatencyMs + bufferMs;

      const directConnectTime1 = directConnectAttemptTime + estimate1.timeDiff;
      const directConnectTime2 = directConnectAttemptTime + estimate2.timeDiff;

      const [success1, success2] = await Promise.all([
        peer1.attemptDirectConnect(directConnectTime1),
        peer2.attemptDirectConnect(directConnectTime2),
      ]);

      if (cancelled === true) return;

      return resolve(success1 && success2);
    });
  };

  private configureRelayedConnection = (
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ) => {
    peer1.enableRelay();
    peer2.enableRelay();
  };

  private isSessionValidToJoin = (session: Session, key: string): boolean => {
    // Check session has not expired
    if (Date.now() - session.createdAt.getTime() > 86400) {
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
