import * as tls from 'tls';
import * as stream from 'stream';
import * as chalk from 'chalk';
import { DebugClientConfig } from './config';
import {
  KeyAcceptedPayload,
  TlsRelayMessageSerialiser,
  TlsRelayServerMessageType,
  TlsRelayClientMessageType,
  ClientTimePayload,
  TlsRelayClientJsonMessage,
  TlsRelayClientMessage,
  ServerDirectConnectAttemptPayload,
  ServerPeerJoinedPayload,
  TlsRelayServerMessage,
} from '@timetoogo/debug-my-pipeline--shared';
import { DirectConnectionConfig } from './connection-strategies';
import { RelaySocket } from './relay-socket';
import { Ssh } from './ssh';

const COLOURS = {
  info: chalk.white,
  success: chalk.green,
  warning: chalk.yellow,
  error: chalk.red,
};

export class DebugClient {
  private readonly serialiser = new TlsRelayMessageSerialiser();

  private waitingForTypes: TlsRelayServerMessageType[] | null = null;
  private waitingForResolve: Function | null = null;
  private waitingForReject: Function | null = null;
  private timeouts: NodeJS.Timeout[] = [];

  private relaySocket: tls.TLSSocket;
  private peerInfo: ServerPeerJoinedPayload;
  private peerSocket: stream.Duplex;
  private keyType: KeyAcceptedPayload['keyType'];

  private isRelayMode = false;
  private closed = false;

  constructor(private readonly config: DebugClientConfig) {}

  public connect = async (): Promise<void> => {
    try {
      await this.connectToRelayServer();

      await this.waitForPeerToConnect();

      await this.negotiatePeerConnection();

      await this.setupSshConnection();

      await this.close();
    } catch (e) {
      console.log(COLOURS.error(e?.message ? e.message : e));
      return;
    }
  };

  private connectToRelayServer = async (): Promise<void> => {
    console.log(COLOURS.info(`Connecting to ${this.config.relayHost}:${this.config.relayPort}`));

    const socket = tls.connect({
      host: this.config.relayHost,
      port: this.config.relayPort,
      requestCert: true,
      rejectUnauthorized: this.config.verifyHostName,
    });

    this.initSocket(socket);

    this.sendRelayMessage({
      type: TlsRelayClientMessageType.KEY,
      length: this.config.clientKey.length,
      data: Buffer.from(this.config.clientKey, 'utf8'),
    });

    const message = await this.waitFor([
      TlsRelayServerMessageType.KEY_ACCEPTED,
      TlsRelayServerMessageType.KEY_REJECTED,
    ]);

    if (message.type === TlsRelayServerMessageType.KEY_ACCEPTED) {
      const payload = JSON.parse(message.data.toString('utf8')) as KeyAcceptedPayload;
      this.keyType = payload.keyType;
    } else {
      throw new Error(`The key has expired or is invalid`);
    }
  };

  private waitForPeerToConnect = async (): Promise<void> => {
    console.log(COLOURS.info(`Waiting for peer to connect to session...`));

    const message = await this.waitFor([TlsRelayServerMessageType.PEER_JOINED]);
    this.peerInfo = JSON.parse(message.data.toString('utf8')) as ServerPeerJoinedPayload;

    console.log(COLOURS.info(`${this.peerInfo.peerIpAddress} joined the session`));
  };

  private negotiatePeerConnection = async (): Promise<stream.Duplex> => {
    console.log(COLOURS.info(`Negotiating connection...`));

    while (true) {
      const message = await this.waitFor([
        TlsRelayServerMessageType.TIME_PLEASE,
        TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT,
        TlsRelayServerMessageType.START_RELAY_MODE,
      ]);

      switch (message.type) {
        case TlsRelayServerMessageType.TIME_PLEASE:
          await this.sendClientTime();
          break;

        case TlsRelayServerMessageType.ATTEMPT_DIRECT_CONNECT:
          this.peerSocket = await this.attemptDirectConnection(
            JSON.parse(message.data.toString('utf8')) as ServerDirectConnectAttemptPayload,
          );

          await this.sendRelayMessage({
            type: this.peerSocket
              ? TlsRelayClientMessageType.DIRECT_CONNECT_SUCCEEDED
              : TlsRelayClientMessageType.DIRECT_CONNECT_FAILED,
            length: 0,
          });
          break;

        case TlsRelayServerMessageType.START_RELAY_MODE:
          this.peerSocket = this.setupRelaySocket();
          break;

        default:
          throw new Error(`Unreachable`);
          break;
      }

      if (this.peerSocket) {
        return;
      }
    }
  };

  private sendClientTime = async () => {
    await this.sendRelayJsonMessage({
      type: TlsRelayClientMessageType.TIME,
      data: { clientTime: Date.now() },
    });
  };

  private attemptDirectConnection = async (
    payload: ServerDirectConnectAttemptPayload,
  ): Promise<stream.Duplex | undefined> => {
    console.log(COLOURS.info(`Attempting direct connection to ${this.peerInfo.peerIpAddress}...`));

    const timeFromNowToConnect = Math.max(0, payload.connectAt - Date.now());

    const config: DirectConnectionConfig = {
      ipAddress: this.peerInfo.peerIpAddress,
    };

    return new Promise((resolve) => {
      setTimeout(async () => {
        const sockets = await Promise.all(this.config.directConnectStrategies.map((i) => i.attemptConnection(config)));

        const firstSocket = sockets.find((i) => !!i);
        sockets.filter((i) => i && i !== firstSocket).forEach((i) => i.end(i.destroy));

        if (firstSocket) {
          console.log(COLOURS.success(`Successfully connected to ${this.peerInfo.peerIpAddress}`));
        }

        resolve(firstSocket || undefined);
      }, timeFromNowToConnect);
    });
  };

  private setupRelaySocket = (): stream.Duplex => {
    console.log(COLOURS.info(`Falling back to relayed connection`));
    this.isRelayMode = true;
    return new RelaySocket(this.relaySocket);
  };

  private setupSshConnection = async (): Promise<void> => {
    if (this.keyType === 'host') {
      console.log(COLOURS.info(`Waiting for incoming SSH connection`));
      await new Ssh({
        socket: this.peerSocket,
        username: 'debug',
        password: this.config.clientKey,
      }).setupSshServer();
    } else {
      console.log(COLOURS.info(`Connecting to host over SSH`));
      await new Ssh({
        socket: this.peerSocket,
        username: 'debug',
        password: this.peerInfo.peerKey,
      }).connectToHostSshSession();
    }
  };

  private initSocket = (relaySocket: tls.TLSSocket) => {
    this.relaySocket = relaySocket;
    this.relaySocket.on('data', this.handleData);
    this.relaySocket.on('end', this.close);
  };

  private handleData = (data: Buffer) => {
    const messages = this.serialiser.deserialiseStream(data);

    for (const message of messages) {
      this.handleMessage(message);
    }
  };

  private handleMessage = async (message: TlsRelayServerMessage) => {
    if (message.type === TlsRelayServerMessageType.CLOSE) {
      await this.close();
      return;
    }

    if (this.isRelayMode && message.type === TlsRelayServerMessageType.RELAY) {
      return;
    }

    if (this.waitingForTypes && this.waitingForTypes.includes(message.type)) {
      this.waitingForResolve(message);
    } else {
      this.handleUnexpectedMessage(message);
    }
  };

  private waitFor = (types: TlsRelayServerMessageType[], timeLimit?: number): Promise<TlsRelayServerMessage> => {
    if (this.waitingForTypes) {
      throw new Error(`Already waiting for message`);
    }

    this.waitingForTypes = types;
    let timeoutId: NodeJS.Timeout;

    return new Promise<TlsRelayServerMessage>((resolve, reject) => {
      this.waitingForResolve = resolve;
      this.waitingForReject = reject;

      if (timeLimit) {
        this.timeouts.push(
          (timeoutId = setTimeout(() => {
            reject(
              new Error(
                `Connection timed out while waiting for ${types
                  .map((i) => TlsRelayServerMessageType[i])
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

  private handleUnexpectedMessage = (message: TlsRelayServerMessage) => {
    throw new Error(
      `Unexpected message received from relay server: ${TlsRelayServerMessageType[message.type] || 'unknown'}`,
    );
  };

  private sendRelayMessage = (message: TlsRelayClientMessage): Promise<void> => {
    return new Promise((resolve, reject) => {
      this.relaySocket.write(this.serialiser.serialise(message), (err) => (err ? reject(err) : resolve()));
    });
  };

  private sendRelayJsonMessage = <TData>(message: TlsRelayClientJsonMessage<TData>): Promise<void> => {
    return new Promise((resolve, reject) => {
      this.relaySocket.write(this.serialiser.serialiseJson(message), (err) => (err ? reject(err) : resolve()));
    });
  };

  private close = async (): Promise<void> => {
    if (this.closed) {
      return;
    }

    this.closed = true;

    if (this.relaySocket.writable) {
      this.sendRelayMessage({ type: TlsRelayClientMessageType.CLOSE, length: 0 });
      this.relaySocket.end(() => this.relaySocket.destroy());
    }

    if (this.peerSocket && this.peerSocket.writable) {
      this.peerSocket.end(() => this.peerSocket.destroy());
    }

    if (this.waitingForReject) {
      this.waitingForReject(new Error(`Connection closed`));
    }
  };
}
