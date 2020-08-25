import { RelayServer } from "./location";

export interface CreateSessionResponse {
  peer1Key: string;
  peer2Key: string;
}

export class ApiClient {
  createSession = async (relayServer: RelayServer): Promise<CreateSessionResponse> => {
    const response = await fetch(`https://${relayServer.domain}/api/sessions`, {
      method: "POST",
    }).then((i) => i.json());

    return {
      peer1Key: response.peer1_key,
      peer2Key: response.peer2_key,
    };
  };
}
