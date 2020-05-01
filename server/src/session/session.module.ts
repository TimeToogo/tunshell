import { Module } from '@nestjs/common';
import { SessionController } from './session.controller';
import { MongooseModule } from '@nestjs/mongoose';
import { SessionSchema } from './session.schema';
import { ServicesModule } from 'src/services/services.module';

const mongooseModule = MongooseModule.forFeature([
  { name: 'Session', schema: SessionSchema },
]);

@Module({
  imports: [ServicesModule, mongooseModule],
  controllers: [SessionController],
  exports: [mongooseModule],
})
export class SessionModule {}
