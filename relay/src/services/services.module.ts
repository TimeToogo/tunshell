import { Module, Global } from '@nestjs/common';
import { keyGeneratorProvider } from './key-generator.service';
import { clockProvider } from './clock.service';
import { loggerProvider } from './logger.service';

@Module({
  providers: [keyGeneratorProvider, clockProvider, loggerProvider],
  exports: [keyGeneratorProvider, clockProvider, loggerProvider],
})
export class ServicesModule {}
