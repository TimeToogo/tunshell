import { Module } from '@nestjs/common';
import { ScriptController } from './script.controller';

@Module({
  controllers: [ScriptController],
})
export class ScriptModule {}
