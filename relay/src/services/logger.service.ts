import { Injectable, Scope, Logger, ClassProvider } from '@nestjs/common';

@Injectable({ scope: Scope.TRANSIENT })
export class MyLogger extends Logger {}

export const loggerProvider: ClassProvider<MyLogger> = {
  provide: 'Logger',
  useClass: MyLogger,
};
