import { DirectConnectionStrategy } from './connection-strategies';

export interface DebugClientConfig {
  relayHost: string;
  relayPort: number;
  clientKey: string;
  verifyHostName: boolean;
  directConnectStrategies: DirectConnectionStrategy[];
}

export const getDefaultConfig = (): DebugClientConfig => ({
  relayHost: 'relay.debugmypipeline.com',
  relayPort: 3001,
  clientKey: process.env.DEBUGMYPIPELINE_KEY,
  verifyHostName: true,
  directConnectStrategies: [],
});
