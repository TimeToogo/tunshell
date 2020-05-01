import * as mongoose from 'mongoose';

export interface Participant {
  key: string;
  joined: boolean;
  ipAddress: string | null;
}

export interface Session {
  host: Participant;
  client: Participant;
  createdAt: Date;
}

export interface CreateSessionResponse {
  hostKey: string;
  clientKey: string;
}
