import { Module } from '@nestjs/common';
import { tlsConfigProvider } from './relay.config';
import { tlsRelayServerProvider } from './relay.server';
import { SessionModule } from '../session/session.module';
import { ServicesModule } from '../services/services.module';

@Module({
  imports: [SessionModule, ServicesModule],
  providers: [tlsConfigProvider, tlsRelayServerProvider],
  exports: [tlsRelayServerProvider],
})
export class TlsModule {}
