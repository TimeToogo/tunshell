export interface CreateSessionResponse {
  peer1Key: string;
  peer2Key: string;
}

export class ApiClient {
  createSession = async (): Promise<CreateSessionResponse> => {
    const response = await fetch("https://relay.tunshell.com/api/sessions", {
      method: "POST",
    }).then((i) => i.json());

    return {
      peer1Key: response.peer1_key,
      peer2Key: response.peer2_key,
    };
  };
}
