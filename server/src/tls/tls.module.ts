import { Module } from '@nestjs/common';
import { tlsConfigProvider } from './tls.config';
import { tlsRelayServiceProvider } from './tls.service';
import { SessionModule } from 'src/session/session.module';
import { ServicesModule } from 'src/services/services.module';

@Module({
  imports: [SessionModule, ServicesModule],
  providers: [tlsConfigProvider, tlsRelayServiceProvider],
  exports: [tlsRelayServiceProvider],
})
export class TlsModule {}
