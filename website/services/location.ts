export interface RelayServer {
  label: string;
  domain: string;
  default: boolean;
}

export const RELAY_SERVERS: RelayServer[] = [
  { label: "US", domain: "relay.tunshell.com", default: true },
  { label: "AU", domain: "au.relay.tunshell.com", default: false },
  { label: "UK", domain: "eu.relay.tunshell.com", default: false },
];

export class LocationService {
  public findNearestRelayServer = async (): Promise<RelayServer> => {
    const response = await fetch("https://nearest.relay.tunshell.com/api/info").then((i) => i.json());

    return RELAY_SERVERS.find((i) => i.domain === response.domain_name);
  };
}
