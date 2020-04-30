import * as tls from 'tls';
import * as fs from 'fs';
import { ValueProvider } from '@nestjs/common';

export interface TlsRelayConfig {
  server: tls.TlsOptions;

  connection: {
    waitForKeyTimeout: number;
  };
}

const TlsConfig: TlsRelayConfig = {
  server: {
    key: fs.readFileSync(process.env.TLS_RELAY_PRIVATE_KEY),
    cert: fs.readFileSync(process.env.TLS_RELAY_CERT),
  },

  connection: {
    waitForKeyTimeout: 5000,
  },
};

export const tlsConfigProvider: ValueProvider<TlsRelayConfig> = {
  provide: 'TlsConfig',
  useValue: TlsConfig,
};
