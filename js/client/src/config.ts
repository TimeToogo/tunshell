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
  if (!process.env.DEBUGMYPIPELINE_KEY) {
    console.error(chalk.red(`Could not find DEBUGMYPIPELINE_KEY env var`));
    process.exit(1);
  }

  return {
    relayHost: 'relay1.debugmypipeline.com',
    relayPort: 5000,
    clientKey: process.env.DEBUGMYPIPELINE_KEY,
    verifyHostName: true,
    directConnectStrategies: [],
  };
};
