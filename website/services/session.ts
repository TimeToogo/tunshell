import { ApiClient, CreateSessionResponse } from "./api-client";
import { RelayServer } from "./location";

export interface SessionKeys {
  targetKey: string;
  localKey: string;
  encryptionSecret: string;
  relayServer: RelayServer;
}

export class SessionService {
  private readonly api = new ApiClient();

  public createSessionKeys = async (relayServer: RelayServer): Promise<SessionKeys> => {
    let keys = await this.api.createSession(relayServer);
    keys = this.randomizeSessionKeys(keys);

    return {
      targetKey: keys.peer1Key,
      localKey: keys.peer2Key,
      encryptionSecret: this.generateEncryptionSecret(),
      relayServer,
    };
  };

  // To ensure the relay server does not know which
  // key is assigned to the local/target hosts we
  // randomise them here
  private randomizeSessionKeys = (keys: CreateSessionResponse): CreateSessionResponse => {
    const flip = Math.random() >= 0.5;

    return flip
      ? {
          peer1Key: keys.peer2Key,
          peer2Key: keys.peer1Key,
        }
      : {
          peer1Key: keys.peer1Key,
          peer2Key: keys.peer2Key,
        };
  };

  // Generates a secure secret for each of the clients
  // Key: 22 alphanumeric chars (131 bits of entropy)
  private generateEncryptionSecret = (): string => {
    const alphanumeric = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    const gen = (len: number): string => {
      const buff = new Uint8Array(len);
      window.crypto.getRandomValues(buff);
      let out = "";

      for (let i = 0; i < len; i++) {
        out += alphanumeric[buff[i] % alphanumeric.length];
      }

      return out;
    };

    return gen(22);
  };
}
