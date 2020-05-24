import { DebugClient } from './client';
import { getDefaultConfig } from './config';
import chalk = require('chalk');

const run = async () => {
  try {
    await new DebugClient(getDefaultConfig()).connect();
    process.exit(0);
  } catch (e) {
    console.error(chalk.red(`Unhandled error occurred: ${e.message}`));
    process.exit(1);
  }
};

run();
