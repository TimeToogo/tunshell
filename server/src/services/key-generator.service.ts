import * as uuid from 'uuid';
import { ClassProvider } from '@nestjs/common';

export interface IKeyGenerator {
  generate(): string;
}

export class UuidV4KeyGenerator implements IKeyGenerator {
  generate(): string {
    return uuid.v4();
  }
}

export const keyGeneratorProvider: ClassProvider<IKeyGenerator> = {
  provide: 'IKeyGenerator',
  useClass: UuidV4KeyGenerator,
};