import { NestFactory } from '@nestjs/core';
import { AppModule } from './app.module';
import { TlsRelayService } from './tls/tls.service';

async function bootstrap() {
  const app = await NestFactory.create(AppModule);
  const tlsRelay = app.get<TlsRelayService>('TlsRelayService');

  await Promise.all([app.listen(3000), tlsRelay.listen(3001)]);
}
bootstrap();
