import { EventEmitter } from 'events';
import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session } from '../session/session.model';
import * as tls from 'tls';
import { ClassProvider, Inject, LoggerService, Logger } from '@nestjs/common';
import { TlsRelayConfig } from './relay.config';
import { TlsRelayConnectionState, LatencyEstimation } from './relay.model';
import {
  TlsRelayMessageSerialiser,
  TlsRelayClientMessage,
  TlsRelayServerMessage,
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  ClientTimePayload,
  TlsRelayServerJsonMessage,
  ServerDirectConnectAttemptPayload,
  ServerPeerJoinedPayload,
  KeyAcceptedPayload,
  TlsRelayMessageDuplexStream,
} from '@timetoogo/debug-my-pipeline--shared';

export class TlsRelayConnection extends EventEmitter {
  private state: TlsRelayConnectionState = TlsRelayConnectionState.NEW;
  private readonly messageStream: TlsRelayMessageDuplexStream<TlsRelayServerMessageType, TlsRelayClientMessageType>;
  private timeouts: NodeJS.Timeout[] = [];
  private serialiser: TlsRelayMessageSerialiser = new TlsRelayMessageSerialiser();
  private key: string | null = null;
  private session: (Session & Document) | null = null;
  private peer: TlsRelayConnection | null = null;

  private isProcessingMessages = false;
  private messageQueue: TlsRelayClientMessage[] = [];

  private waitingForTypes: TlsRelayClientMessageType[] | null = null;
  private waitingForResolve: Function | null = null;
  private waitingForReject: Function | null = null;

  constructor(
    private readonly config: TlsRelayConfig['connection'],
    private readonly socket: tls.TLSSocket,
    @Inject('Logger') private readonly logger: LoggerService,
  ) {
    super();
    this.messageStream = new TlsRelayMessageDuplexStream(socket, TlsRelayServerMessageType, TlsRelayClientMessageType);
    this.init();
  }

  public getState = () => this.state;
  public getKey = () => this.key;
  public getSession = () => this.session;
  public getSocket = () => this.socket;
  public isHost = () => this.session.host.key === this.key;
  public isClient = () => this.session.client.key === this.key;
  public getParticipant = () => {
    if (!this.session) {
      return null;
    }

    return this.session.host.key === this.key ? this.session.host : this.session.client;
  };

  isDead = () => this.state === TlsRelayConnectionState.CLOSED;

  private init = () => {
    this.messageStream.on('data', this.handleData);

    this.messageStream.on('error', this.handleError);

    this.messageStream.on('close', this.handleSocketClose);
  };

  public waitForKey = () => {
    this.state = TlsRelayConnectionState.WAITING_FOR_KEY;
    this.waitFor([TlsRelayClientMessageType.KEY], this.config.waitForKeyTimeout)
      .then(this.handleKey)
      .catch(() => {});
  };

  private handleData = (message: TlsRelayClientMessage) => {
    this.messageQueue.push(message);
    this.startMessageProcessingLoop();
  };

  private startMessageProcessingLoop = async () => {
    if (this.isProcessingMessages) {
      return;
    }

    this.isProcessingMessages = true;

    let message: TlsRelayClientMessage | null;
    while ((message = this.messageQueue.shift())) {
      try {
        await this.handleMessage(message);
      } catch (e) {
        this.logger.error(`Error occurred during handling of message data: ${e.message}`, e.stack);
      }
    }

    this.isProcessingMessages = false;
  };

  private handleMessage = async (message: TlsRelayClientMessage) => {
    if (message.type === TlsRelayClientMessageType.CLOSE) {
      await this.close();
      return;
    }

    if (this.state === TlsRelayConnectionState.RELAYED_CONNECTION && message.type === TlsRelayClientMessageType.RELAY) {
      await this.handleRelayMessage(message);
      return;
    }

    if (this.waitingForTypes && this.waitingForTypes.includes(message.type)) {
      this.waitingForResolve(message);
    } else {
      this.handleUnexpectedMessage(message);
    }
  };

  private waitFor = (types: TlsRelayClientMessageType[], timeLimit?: number): Promise<TlsRelayClientMessage> => {
    if (this.waitingForTypes) {
      throw new Error(`Already waiting for message`);
    }

    this.waitingForTypes = types;
    let timeoutId: NodeJS.Timeout;

    return new Promise<TlsRelayClientMessage>((resolve, reject) => {
      this.waitingForResolve = resolve;
      this.waitingForReject = reject;

      if (timeLimit) {
        this.timeouts.push(
          (timeoutId = setTimeout(() => {
            reject(
              new Error(
                `Connection timed out while waiting for ${types
                  .map(i => TlsRelayClientMessageType[i])
                  .join(', ')} messages`,
              ),
            );
            this.close();
          }, timeLimit)),
        );
      }
    }).finally(() => {
      this.waitingForTypes = null;
      this.waitingForResolve = null;
      this.waitingForReject = null;
      clearTimeout(timeoutId);
    });
  };

  private handleKey = (message: TlsRelayClientMessage) => {
    if (this.state !== TlsRelayConnectionState.WAITING_FOR_KEY) {
      throw new Error(`Connection is not in WAITING_FOR_KEY state`);
    }

    if (message.length < 16 || message.length > 64) {
      this.handleError(new Error(`Key does not meet data length requirements, received length: ${message.length}`));
      return;
    }

    this.key = message.data.toString('utf8');
    this.emit('key-received', this.key);
  };

  public acceptKey = async (session: Session & Document) => {
    this.session = session;
    const participant = this.getParticipant();
    participant.ipAddress = this.socket.remoteAddress;
    participant.joined = true;
    await this.session.save();
    await this.sendJsonMessage<KeyAcceptedPayload>({
      type: TlsRelayServerMessageType.KEY_ACCEPTED,
      data: {
        keyType: this.isHost() ? 'host' : 'client',
      },
    });
    this.state = TlsRelayConnectionState.WAITING_FOR_PEER;
    this.timeouts.push(setTimeout(this.timeoutWaitingForPeer, this.config.waitForPeerTimeout));
  };

  public rejectKey = async () => {
    this.state = TlsRelayConnectionState.KEY_INVALID;
    await this.sendMessage({
      type: TlsRelayServerMessageType.KEY_REJECTED,
      length: 0,
    });
    await this.close();
  };

  public estimateLatency = async (): Promise<LatencyEstimation> => {
    // TODO: NTP sync
    const timePleaseMessage: TlsRelayServerMessage = {
      type: TlsRelayServerMessageType.TIME_PLEASE,
      length: 0,
    };

    await this.sendMessage(timePleaseMessage);
    const requestSentTime = Date.now();

    const message = await this.waitFor([TlsRelayClientMessageType.TIME], this.config.estimateLatencyTimeout);

    const receivedResponseTime = Date.now();
    const timeMessage = this.serialiser.deserialiseJson<ClientTimePayload>(message);

    if (typeof timeMessage.data.clientTime !== 'number' || timeMessage.data.clientTime < 0) {
      throw new Error(`Client returned time payload with incorrect structure: ${JSON.stringify(timeMessage)}`);
    }

    const roundTripLatency = receivedResponseTime - requestSentTime;
    // TODO: Improve upon naive assumption of symmetrical outbound/inbound latency
    const sendLatency = roundTripLatency / 2;
    const receiveLatency = roundTripLatency / 2;
    const timeDiff = requestSentTime + sendLatency - timeMessage.data.clientTime;

    return {
      sendLatency,
      receiveLatency,
      timeDiff,
    };
  };

  public attemptDirectConnect = async (coordinatedPeerTime: number): Promise<boolean> => {
    await this.sendJsonMessage<ServerDirectConnectAttemptPayload>({
      type: TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
      data: {
        connectAt: coordinatedPeerTime,
      },
    });

    const message = await this.waitFor(
      [TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED, TlsRelayClientMessageType.DIRECT_CONNECT_FAILED],
      this.config.attemptDirectConnectTimeout,
    );

    if (message.type === TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED) {
      this.state = TlsRelayConnectionState.DIRECT_CONNECTION;
      this.timeouts.push(setTimeout(this.timeoutDirectConnection, this.config.connectionTimeLimit));
      return true;
    } else {
      this.state = TlsRelayConnectionState.DIRECT_CONNECT_FAILED;
      return false;
    }
  };

  private timeoutDirectConnection = async () => {
    if (this.state === TlsRelayConnectionState.DIRECT_CONNECTION) {
      this.logger.warn(`Direct connection expired`);
      await this.close();
    }
  };

  public enableRelay = async () => {
    await this.sendMessage({
      type: TlsRelayServerMessageType.START_RELAY_MODE,
      length: 0,
    });
    this.state = TlsRelayConnectionState.RELAYED_CONNECTION;
    this.timeouts.push(setTimeout(this.timeoutRelay, this.config.connectionTimeLimit));
  };

  private handleRelayMessage = async (message: TlsRelayClientMessage) => {
    await this.peer.sendMessage({
      type: TlsRelayServerMessageType.RELAY,
      length: message.length,
      data: message.data,
    });
  };

  private timeoutRelay = async () => {
    if (this.state === TlsRelayConnectionState.RELAYED_CONNECTION) {
      this.logger.warn(`Relay connection expired`);
      await this.close();
    }
  };

  public peerJoined = async (peer: TlsRelayConnection) => {
    if (this.state !== TlsRelayConnectionState.WAITING_FOR_PEER) {
      throw new Error(`Not waiting for peer`);
    }

    this.peer = peer;

    await this.sendJsonMessage<ServerPeerJoinedPayload>({
      type: TlsRelayServerMessageType.PEER_JOINED,
      data: {
        peerKey: peer.getKey(),
        peerIpAddress: this.peer.getSocket().remoteAddress,
      },
    });

    this.state = TlsRelayConnectionState.NEGOTIATING_CONNECTION;
  };

  private timeoutWaitingForPeer = () => {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_PEER) {
      this.logger.warn(`Timeout waiting for peer`);
      this.close();
    }
  };

  private sendMessage = (message: TlsRelayServerMessage): Promise<void> => {
    if (!this.messageStream.writable) {
      throw new Error(`Cannot send message, socket is not writable`);
    }

    return new Promise<void>((resolve, reject) =>
      this.messageStream.write(message, err => (err ? reject(err) : resolve())),
    );
  };

  private sendJsonMessage = async <T>(message: TlsRelayServerJsonMessage<T>) => {
    if (!this.messageStream.writable) {
      throw new Error(`Cannot send message, socket is not writable`);
    }

    return new Promise<void>((resolve, reject) =>
      this.messageStream.writeJson(message, null, err => (err ? reject(err) : resolve())),
    );
  };

  private handleUnexpectedMessage = (message: TlsRelayClientMessage) => {
    this.handleError(
      new Error(
        `Unexpected message received while in connection state ${
          TlsRelayConnectionState[this.state]
        }: ${TlsRelayClientMessageType[message.type] || 'unknown'}`,
      ),
    );
  };

  private handleError = (error?: Error) => {
    if (error) {
      this.logger.error(`TLS Relay error received: ${error.message}`);
      this.close();
    }
  };

  public close = async () => {
    if (this.state === TlsRelayConnectionState.CLOSED) {
      return;
    }

    if (this.messageStream.writable) {
      await this.sendMessage({
        type: TlsRelayServerMessageType.CLOSE,
        length: 0,
      });
    }

    if (this.waitingForReject) {
      this.waitingForReject(new Error(`Connection closed`));
    }

    this.state = TlsRelayConnectionState.CLOSED;
    this.messageStream.end(() => {
      this.messageStream.destroy();
    });

    await this.cleanUp();
  };

  private handleSocketClose = async () => {
    this.state = TlsRelayConnectionState.CLOSED;

    if (this.peer) {
      this.peer.notifyPeerClosed();
    }

    await this.cleanUp();
  };

  private cleanUp = async () => {
    if (this.getParticipant() && this.getParticipant().joined) {
      this.getParticipant().joined = false;
      await this.session.save();
    }

    this.timeouts.forEach(clearTimeout);
    this.timeouts = [];
  }

  private notifyPeerClosed = () => {
    this.close();
  };
}
