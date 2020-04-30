import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session } from '../session/session.model';
import * as tls from 'tls';
import { ClassProvider, Inject, LoggerService, Logger } from '@nestjs/common';
import { TlsRelayConfig } from './tls.config';
import { TlsRelayConnectionState } from './tls.model';

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

    connection.onKey(key => {});
  }
}

export class TlsRelayConnection {
  private state: TlsRelayConnectionState = TlsRelayConnectionState.NEW;

  constructor(
    private readonly config: TlsRelayConfig['connection'],
    private readonly socket: tls.TLSSocket,
    @Inject('Logger') private readonly logger: LoggerService,
  ) {}

  public getState = () => this.state;

  public async init() {
    this.socket.on('data', this.handleData);

    this.state = TlsRelayConnectionState.WAITING_FOR_KEY;

    setTimeout(this.timeoutWaitingForKey, this.config.waitForKeyTimeout);
  }

  private timeoutWaitingForKey() {
    if (this.state === TlsRelayConnectionState.WAITING_FOR_KEY) {
      this.state = TlsRelayConnectionState.EXPIRED_WAITING_FOR_KEY;
    }
  }

  private handleData(data: Buffer) {
    throw new Error("Method not implemented.");
  }
}

export const tlsRelayServiceProvider: ClassProvider<TlsRelayService> = {
  provide: 'TlsRelayService',
  useClass: TlsRelayService,
};
