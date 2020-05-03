import { DebugClient } from './client';
import { getDefaultConfig } from './config';

new DebugClient(getDefaultConfig()).connect();
