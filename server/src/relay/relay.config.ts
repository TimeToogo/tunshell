import * as tls from 'tls';
import * as fs from 'fs';
import { ValueProvider } from '@nestjs/common';

export interface TlsRelayConfig {
  server: tls.TlsOptions;

  cleanUpInterval: number;

  connection: {
    waitForKeyTimeout: number;
    waitForPeerTimeout: number;
    estimateLatencyTimeout: number;
    attemptDirectConnectTimeout: number;
    connectionTimeLimit: number;
  };
}

const TlsConfig: TlsRelayConfig = {
  server: {
    key: fs.readFileSync(process.env.TLS_RELAY_PRIVATE_KEY),
    cert: fs.readFileSync(process.env.TLS_RELAY_CERT),
  },

  cleanUpInterval: 60 * 1000,

  connection: {
    waitForKeyTimeout: 5 * 1000,
    waitForPeerTimeout: 600 * 1000,
    estimateLatencyTimeout: 5 * 1000,
    attemptDirectConnectTimeout: 10 * 1000,
    connectionTimeLimit: 3600 * 1000,
  },
};

export const tlsConfigProvider: ValueProvider<TlsRelayConfig> = {
  provide: 'TlsConfig',
  useValue: TlsConfig,
};
