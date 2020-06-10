import { DirectConnectionStrategy } from './connection-strategies';
import chalk = require('chalk');

export interface DebugClientConfig {
  relayHost: string;
  relayPort: number;
  clientKey: string;
  verifyHostName: boolean;
  directConnectStrategies: DirectConnectionStrategy[];
}

export const getDefaultConfig = (): DebugClientConfig => {
  if (!process.env.TUNSHELL_KEY) {
    console.error(chalk.red(`Could not find TUNSHELL_KEY env var`));
    process.exit(1);
  }

  return {
    relayHost: 'relay.tunshell.com',
    relayPort: 5000,
    clientKey: process.env.TUNSHELL_KEY,
    verifyHostName: true,
    directConnectStrategies: [],
  };
};
