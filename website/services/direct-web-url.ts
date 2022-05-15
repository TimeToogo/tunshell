import { RELAY_SERVERS } from "./location";
import { SessionKeys } from "./session";

export class WebUrlService {
  public createWebUrl = (session: SessionKeys): string => {
    return `${window.location.origin}/term#${session.localKey},${session.encryptionSecret},${session.relayServer.domain}`;
  };

  public parseWebUrl = (urlString: string): SessionKeys | null => {
    const url = new URL(urlString);

    if (!url.hash) {
      return null;
    }

    const parts = url.hash.substring(1).split(",");

    if (parts.length < 3) {
      return null;
    }

    const [localKey, encryptionSecret, relayServerDomain] = parts;
    const relayServer = RELAY_SERVERS.find((i) => i.domain === relayServerDomain);

    if (!relayServer) {
      return null;
    }

    return { localKey, targetKey: "", encryptionSecret, relayServer };
  };
}
