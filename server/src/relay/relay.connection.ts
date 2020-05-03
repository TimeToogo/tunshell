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
} from '@timetoogo/debug-my-pipeline--shared';

export class TlsRelayConnection extends EventEmitter {
  private state: TlsRelayConnectionState = TlsRelayConnectionState.NEW;
  private serialiser: TlsRelayMessageSerialiser = new TlsRelayMessageSerialiser();
  private key: string | null = null;
  private session: (Session & Document) | null = null;
  private peer: TlsRelayConnection | null = null;

  private waitingForTypes: TlsRelayClientMessageType[] | null = null;
  private waitingForResolve: Function | null = null;
  private waitingForReject: Function | null = null;

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

  public init = async () => {
    this.socket.on('data', this.handleData);

    this.socket.on('close', this.handleSocketClose);

    this.state = TlsRelayConnectionState.WAITING_FOR_KEY;
    this.waitFor([TlsRelayClientMessageType.KEY]).then(this.handleKey).catch(() => {});

    setTimeout(this.timeoutWaitingForKey, this.config.waitForKeyTimeout);
  };

  private timeoutWaitingForKey = () => {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_KEY) {
      this.state = TlsRelayConnectionState.EXPIRED_WAITING_FOR_KEY;
      this.close();
    }
  };

  private handleData = (data: Buffer) => {
    this.handleMessage(
      this.serialiser.deserialise<TlsRelayClientMessage>(data),
    );
  };

  private handleMessage = (message: TlsRelayClientMessage) => {
    if (message.type === TlsRelayClientMessageType.CLOSE) {
      this.close();
      return;
    }

    if (this.waitingForTypes.includes(message.type)) {
      this.waitingForResolve(message);
    } else {
      this.handleUnexpectedMessage(message);
    }
  };

  private waitFor = (
    types: TlsRelayClientMessageType[],
  ): Promise<TlsRelayClientMessage> => {
    if (this.waitingForTypes) {
      throw new Error(`Already waiting for message`);
    }

    this.waitingForTypes = types;

    return new Promise<TlsRelayClientMessage>((resolve, reject) => {
      this.waitingForResolve = resolve;
      this.waitingForReject = reject;
    }).finally(() => {
      this.waitingForTypes = null;
      this.waitingForResolve = null;
      this.waitingForReject = null;
    });
  };

  private handleKey = (message: TlsRelayClientMessage) => {
    if (this.state !== TlsRelayConnectionState.WAITING_FOR_KEY) {
      throw new Error(`Connection is not in WAITING_FOR_KEY state`);
    }

    if (message.length < 16 || message.length > 64) {
      this.handleError(
        new Error(
          `Key does not meet data length requirements, received length: ${message.length}`,
        ),
      );
      return;
    }

    this.key = message.data.toString('utf8');
    this.emit('key-received', this.key);
  };

  public acceptKey = async (session: Session & Document) => {
    this.state = TlsRelayConnectionState.WAITING_FOR_PEER;
    this.session = session;
    const participant = this.getParticipant();
    participant.ipAddress = this.socket.remoteAddress;
    participant.joined = true;
    await this.session.save();
    setTimeout(this.timeoutWaitingForPeer, this.config.waitForPeerTimeout);
  };

  public rejectKey = () => {
    this.state = TlsRelayConnectionState.KEY_INVALID;
    this.sendMessage({
      type: TlsRelayServerMessageType.KEY_REJECTED,
      length: 0,
    });
    this.close();
  };

  public estimateLatency = async (): Promise<LatencyEstimation> => {
    // TODO: NTP sync
    const timePleaseMessage: TlsRelayServerMessage = {
      type: TlsRelayServerMessageType.TIME_PLEASE,
      length: 0,
    };

    this.sendMessage(timePleaseMessage);
    const requestSentTime = Date.now();

    this.state = TlsRelayConnectionState.NEGOTIATING__WAITING_FOR_TIME;
    const message = await this.waitFor([TlsRelayClientMessageType.TIME]);

    const receivedResponseTime = Date.now();
    const timeMessage = this.serialiser.deserialiseJson<ClientTimePayload>(
      message,
    );

    if (
      typeof timeMessage.data.clientTime !== 'number' ||
      timeMessage.data.clientTime < 0
    ) {
      throw new Error(
        `Client returned time payload with incorrect structure: ${JSON.stringify(
          timeMessage,
        )}`,
      );
    }

    const roundTripLatency = receivedResponseTime - requestSentTime;
    // TODO: Remove naive assumption of symmetrical outbound/inbound latency
    const sendLatency = roundTripLatency / 2;
    const receiveLatency = roundTripLatency / 2;
    const timeDiff =
      requestSentTime + sendLatency - timeMessage.data.clientTime;

    return {
      sendLatency,
      receiveLatency,
      timeDiff,
    };
  };

  public attemptDirectConnect = async (
    coordinatedPeerTime: number,
  ): Promise<boolean> => {
    this.sendJsonMessage<ServerDirectConnectAttemptPayload>({
      type: TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
      data: {
        connectAt: coordinatedPeerTime,
      },
    });

    const message = await this.waitFor([
      TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED,
      TlsRelayClientMessageType.DIRECT_CONNECT_FAILED,
    ]);

    if (message.type === TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED) {
      this.state = TlsRelayConnectionState.DIRECT_CONNECTION;
      setTimeout(this.timeoutDirectConnection, this.config.connectionTimeLimit);
      return true;
    } else {
      this.state = TlsRelayConnectionState.DIRECT_CONNECT_FAILED;
      return false;
    }
  };

  private timeoutDirectConnection = () => {
    if (this.state === TlsRelayConnectionState.DIRECT_CONNECTION) {
      this.state = TlsRelayConnectionState.EXPIRED_CONNECTION;
      this.close();
    }
  };

  public enableRelay = () => {
    this.sendMessage({
      type: TlsRelayServerMessageType.START_RELAY_MODE,
      length: 0,
    });
    this.state = TlsRelayConnectionState.RELAYED_CONNECTION;
    this.waitForRelayMessage();
    setTimeout(this.timeoutRelay, this.config.connectionTimeLimit);
  };

  private waitForRelayMessage = async () => {
    const message = await this.waitFor([TlsRelayClientMessageType.RELAY]);

    this.peer.sendMessage({
      type: TlsRelayServerMessageType.RELAY,
      length: message.length,
      data: message.data,
    });

    this.waitForRelayMessage();
  };

  private timeoutRelay = () => {
    if (this.state === TlsRelayConnectionState.RELAYED_CONNECTION) {
      this.state = TlsRelayConnectionState.EXPIRED_CONNECTION;
      this.close();
    }
  };

  public peerJoined = (peer: TlsRelayConnection) => {
    if (this.state !== TlsRelayConnectionState.WAITING_FOR_PEER) {
      throw new Error(`Not waiting for peer`);
    }

    this.peer = peer;

    this.sendJsonMessage<ServerPeerJoinedPayload>({
      type: TlsRelayServerMessageType.PEER_JOINED,
      data: {
        peerIpAddress: this.peer.socket.remoteAddress,
      },
    });

    this.state = TlsRelayConnectionState.NEGOTIATING_CONNECTION;
  };

  private timeoutWaitingForPeer = () => {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_PEER) {
      this.state = TlsRelayConnectionState.EXPIRED_WAITING_FOR_PEER;
      this.close();
    }
  };

  private sendMessage = (message: TlsRelayServerMessage | Buffer) => {
    if (!this.socket.writable) {
      throw new Error(`Cannot send message, socket is not writable`);
    }

    const buffer =
      message instanceof Buffer ? message : this.serialiser.serialise(message);
    this.socket.write(buffer, this.handleError);
  };

  private sendJsonMessage = <T>(message: TlsRelayServerJsonMessage<T>) => {
    this.sendMessage(this.serialiser.serialiseJson(message));
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
      this.logger.log(
        `TLS Relay error received: ${error.message}`,
        error.stack!,
      );
      this.close();
    }
  };

  public close = () => {
    if (this.state === TlsRelayConnectionState.CLOSED) {
      return;
    }

    if (this.socket.writable) {
      this.sendMessage({ type: TlsRelayServerMessageType.CLOSE, length: 0 });
    }

    if (this.waitingForReject) {
      this.waitingForReject(new Error(`Connection closed`));
    }

    this.state = TlsRelayConnectionState.CLOSED;
    this.socket.end(() => {
      this.socket.destroy()
    });
  };

  private handleSocketClose = () => {
    this.state = TlsRelayConnectionState.CLOSED;

    if (this.peer) {
      this.peer.notifyPeerClosed();
    }
  };

  private notifyPeerClosed = () => {
    this.close();
  };
}

