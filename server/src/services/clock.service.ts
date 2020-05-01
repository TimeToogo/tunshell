import * as uuid from 'uuid';
import { ClassProvider } from '@nestjs/common';

export interface IClock {
  now(): Date;
}

export class DateClock implements IClock {
  now(): Date {
    return new Date();
  }
}

export const clockProvider: ClassProvider<IClock> = {
  provide: 'IClock',
  useClass: DateClock,
};