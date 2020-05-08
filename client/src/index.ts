import { DebugClient } from './client';
import { getDefaultConfig } from './config';
import chalk = require('chalk');

const run = async () => {
  try {
    if (!process.stdin.isTTY) {
      console.error(chalk.red(`Process must be run with a TTY`));
      process.exit(1);
    }

    await new DebugClient(getDefaultConfig()).connect();
    process.exit(0);
  } catch (e) {
    console.error(chalk.red(`Unhandled error occurred: ${e.message}`));
    process.exit(1);
  }
};

run();
