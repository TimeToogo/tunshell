import * as stream from 'stream'

export interface DirectConnectionConfig {
  ipAddress: string;
}

export interface DirectConnectionStrategy {
  attemptConnection(config: DirectConnectionConfig): Promise<stream.Duplex | undefined>;
}
