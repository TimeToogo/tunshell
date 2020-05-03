import * as dotenv from 'dotenv';
dotenv.config();

import { NestFactory } from '@nestjs/core';
import { AppModule } from './app.module';
import { TlsRelayServer } from './relay/relay.server';

async function bootstrap() {
  const app = await NestFactory.create(AppModule);
  const tlsRelay = app.get<TlsRelayServer>('TlsRelayServer');

  await Promise.all([app.listen(3000), tlsRelay.listen(3001)]);
}
bootstrap();
