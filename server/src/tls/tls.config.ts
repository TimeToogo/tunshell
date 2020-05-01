import * as tls from 'tls';
import * as fs from 'fs';
import { ValueProvider } from '@nestjs/common';

export interface TlsRelayConfig {
  server: tls.TlsOptions;

  cleanUpInterval: number;

  connection: {
    waitForKeyTimeout: number;
    waitForPeerTimeout: number;
  };
}

const TlsConfig: TlsRelayConfig = {
  server: {
    key: fs.readFileSync(process.env.TLS_RELAY_PRIVATE_KEY),
    cert: fs.readFileSync(process.env.TLS_RELAY_CERT),
  },

  cleanUpInterval: 60 * 1000,

  connection: {
    waitForKeyTimeout: 5000,
    waitForPeerTimeout: 10 * 60 * 1000,
  },
};

export const tlsConfigProvider: ValueProvider<TlsRelayConfig> = {
  provide: 'TlsConfig',
  useValue: TlsConfig,
};
