import { Controller, Post, Inject } from '@nestjs/common';
import { InjectModel } from '@nestjs/mongoose';
import { Model, Document } from 'mongoose';
import { Session, CreateSessionResponse } from './session.model';
import { IKeyGenerator } from '../services/key-generator.service';
import { IClock } from '../services/clock.service';

@Controller('/sessions')
export class SessionController {
  constructor(
    @InjectModel('Session') private sessionModel: Model<Session & Document>,
    @Inject('IKeyGenerator') private keyGenerator: IKeyGenerator,
    @Inject('IClock') private clock: IClock,
  ) {}

  @Post()
  async createSession(): Promise<CreateSessionResponse> {
    const hostKey = this.keyGenerator.generate();
    const clientKey = this.keyGenerator.generate();

    const session: Session = {
      host: {
        key: hostKey,
        joined: false,
        ipAddress: null,
      },
      client: {
        key: clientKey,
        joined: false,
        ipAddress: null,
      },
      createdAt: this.clock.now(),
    };

    await this.sessionModel.create(session);

    return {
      hostKey,
      clientKey,
    };
  }
}
