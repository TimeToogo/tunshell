import { EventEmitter } from 'events';
import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session } from '../session/session.model';
import * as tls from 'tls';
import { ClassProvider, Inject, LoggerService, Logger } from '@nestjs/common';
import { TlsRelayConfig } from './tls.config';
import { TlsRelayConnectionState } from './tls.model';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessage,
  TlsRelayServerMessage,
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
} from '@timetoogo/debug-my-proxy--shared';

export class TlsRelayService {
  private readonly connections = {} as { [key: string]: TlsRelayConnection };

  constructor(
    @Inject('TlsConfig') private readonly config: TlsRelayConfig,
    @Inject('Logger') private readonly logger: LoggerService,
    @InjectModel('Session') private sessionModel: Model<Session & Document>,
  ) {}

  public async listen(port: number): Promise<void> {
    const server = tls.createServer(this.config.server, this.handleConnection);

    server.listen(port, () => {
      this.logger.log('TLS Relay server bound');
      setInterval(this.cleanUpOldConnections, this.config.cleanUpInterval);
    });
  }

  private handleConnection(socket: tls.TLSSocket) {
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
  }

  private async handleConnectionKey(connection: TlsRelayConnection) {
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
  }

  private negotiateConnection(
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ) {
    peer1.peerJoined(peer2);
    peer2.peerJoined(peer1);

    if (await this.negotiateDirectConnection(peer1, peer2)) {
      return;
    }

    this.configureRelayedConnection(peer1, peer2);
  }

  private async negotiateDirectConnection(
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ): Promise<boolean> {
    // All in milliseconds
    let sendLatency1,
      receiveLatency1,
      timeDiff1,
      sendLatency2,
      receiveLatency2,
      timeDiff2;

    try {
      [
        [sendLatency1, receiveLatency1, timeDiff1],
        [sendLatency2, receiveLatency2, timeDiff2],
      ] = await Promise.all([peer1.estimateLatency(), peer2.estimateLatency()]);
    } catch (e) {
      this.logger.log(
        `Error while estimating peer latencies: ${e.message}`,
        e.stack,
      );
      return false;
    }

    const maxLatencyMs = Math.max(sendLatency1, sendLatency2);
    const bufferMs = 500;
    const directConnectAttemptTime = Date.now() + maxLatencyMs + bufferMs;

    const directConnectTime1 = directConnectAttemptTime + timeDiff1;
    const directConnectTime2 = directConnectAttemptTime + timeDiff2;

    const [success1, success2] = await Promise.all([
      peer1.attemptDirectConnect(directConnectTime1),
      peer2.attemptDirectConnect(directConnectTime2),
    ]);

    return success1 && success2;
  }

  private configureRelayedConnection(
    peer1: TlsRelayConnection,
    peer2: TlsRelayConnection,
  ) {
    peer1.enableRelay();
    peer2.enableRelay();
  }

  private isSessionValidToJoin(session: Session, key: string): boolean {
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
  }

  private cleanUpOldConnections() {
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
  }
}

export class TlsRelayConnection extends EventEmitter {
  private state: TlsRelayConnectionState = TlsRelayConnectionState.NEW;
  private serialiser: TlsRelayMessageSerialiser;
  private key: string | null = null;
  private session: (Session & Document) | null = null;
  private peer: TlsRelayConnection | null = null;

  constructor(
    private readonly config: TlsRelayConfig['connection'],
    private readonly socket: tls.TLSSocket,
    @Inject('Logger') private readonly logger: LoggerService,
  ) {
    super();
  }

  public getState = () => this.state;
  public getKey = () => this.key;
  public getSession = () => this.session;
  public isHost = () => this.session.host.key === this.key;
  public isClient = () => this.session.client.key === this.key;
  public getParticipant = () =>
    this.session.host.key === this.key
      ? this.session.host
      : this.session.client;

  isDead = () =>
    this.state === TlsRelayConnectionState.CLOSED ||
    this.state === TlsRelayConnectionState.EXPIRED_WAITING_FOR_KEY ||
    this.state === TlsRelayConnectionState.EXPIRED_WAITING_FOR_PEER ||
    this.state === TlsRelayConnectionState.EXPIRED_NEGOTIATING_CONNECTION ||
    this.state === TlsRelayConnectionState.EXPIRED_CONNECTION;

  public async init() {
    this.socket.on('data', this.handleData);

    this.socket.on('close', this.handleSocketClose);

    this.state = TlsRelayConnectionState.WAITING_FOR_KEY;

    setTimeout(this.timeoutWaitingForKey, this.config.waitForKeyTimeout);
  }

  private timeoutWaitingForKey() {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_KEY) {
      this.state = TlsRelayConnectionState.EXPIRED_WAITING_FOR_KEY;
      this.close();
    }
  }

  private handleData(data: Buffer) {
    this.handleMessage(
      this.serialiser.deserialise<TlsRelayClientMessage>(data),
    );
  }

  private handleMessage(message: TlsRelayClientMessage) {
    switch ([this.state, message.type].join()) {
      case [
        TlsRelayConnectionState.WAITING_FOR_KEY,
        TlsRelayClientMessageType.KEY,
      ].join():
        this.handleKey(message);
        break;

      default:
        this.handleUnexpectedMessage(message);
    }
  }

  private handleKey(message: TlsRelayClientMessage) {
    if (message.length < 16 || message.length > 64) {
      this.handleError(
        new Error(
          `Key does not meet data length requirements, received length: ${message.length}`,
        ),
      );
    }

    this.key = message.data.toString('utf8');
    this.emit('key-received', this.key);
  }

  public async acceptKey(session: Session & Document) {
    this.state = TlsRelayConnectionState.WAITING_FOR_PEER;
    this.session = session;
    const participant = this.getParticipant();
    participant.ipAddress = this.socket.remoteAddress;
    participant.joined = true;
    await this.session.save();
    setTimeout(this.timeoutWaitingForPeer, this.config.waitForPeerTimeout);
  }

  private timeoutWaitingForPeer() {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_PEER) {
      this.state = TlsRelayConnectionState.EXPIRED_WAITING_FOR_PEER;
      this.close();
    }
  }

  public peerJoined(peer: TlsRelayConnection) {
    if (this.state !== TlsRelayConnectionState.WAITING_FOR_PEER) {
      throw new Error(`Not waiting for peer`);
    }

    this.peer = peer;
    this.state = TlsRelayConnectionState.NEGOTIATING_CONNECTION;
  }

  public rejectKey() {
    this.state = TlsRelayConnectionState.KEY_INVALID;
    this.sendMessage({
      type: TlsRelayServerMessageType.KEY_REJECTED,
      length: 0,
    });
    this.close();
  }

  private sendMessage(message: TlsRelayServerMessage) {
    if (!this.socket.writable) {
      return;
    }

    const buffer = this.serialiser.serialise(message);
    this.socket.write(buffer, this.handleError);
  }

  private handleUnexpectedMessage(message: TlsRelayClientMessage) {
    this.handleError(
      new Error(
        `Unexpected message received while in connection state ${
          TlsRelayConnectionState[this.state]
        }: ${TlsRelayClientMessageType[message.type]}`,
      ),
    );
  }

  private handleError(error?: Error) {
    if (error) {
      this.logger.log(
        `TLS Relay error received: ${error.message}`,
        error.stack!,
      );
      this.close();
    }
  }

  private close() {
    if (this.state === TlsRelayConnectionState.CLOSED) {
      return;
    }

    this.sendMessage({ type: TlsRelayServerMessageType.CLOSE, length: 0 });
    this.state = TlsRelayConnectionState.CLOSED;
    this.socket.end();
  }

  private handleSocketClose() {
    this.state = TlsRelayConnectionState.CLOSED;

    if (this.peer) {
      this.peer.notifyPeerClosed();
    }
  }

  private notifyPeerClosed() {
    this.close();
  }
}

export const tlsRelayServiceProvider: ClassProvider<TlsRelayService> = {
  provide: 'TlsRelayService',
  useClass: TlsRelayService,
};
