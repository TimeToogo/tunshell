import * as mongoose from 'mongoose';

export const ParticipantSchema = new mongoose.Schema({
  key: String,
  joined: Boolean,
  ipAddress: {
      type: String,
      default: null
  },
});

export const SessionSchema = new mongoose.Schema({
  host: ParticipantSchema,
  client: ParticipantSchema,
  createdAt: Date
});
