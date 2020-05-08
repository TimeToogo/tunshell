import { Module } from '@nestjs/common';
import { MongooseModule } from '@nestjs/mongoose';
import { SessionModule } from './session/session.module';
import { TlsModule } from './relay/relay.module';
import { ScriptModule } from './script/script.module';

@Module({
  imports: [
    MongooseModule.forRoot(`mongodb://${process.env.MONGO_HOST}:27017/relay`, {
      user: process.env.MONGO_USER,
      pass: process.env.MONGO_PASS,
      useNewUrlParser: true,
      useUnifiedTopology: true
    }),
    SessionModule,
    TlsModule,
    ScriptModule,
  ],
})
export class AppModule {}
