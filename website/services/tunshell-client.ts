import { TerminalEmulator } from "./wasm/tunshell_client";

type ClientModule = typeof import("./wasm/tunshell_client");

export class TunshellClient {
  private module: ClientModule;

  constructor() {}

  init = async () => {
    let module = await import("./wasm/tunshell_client");
    this.module = module;
    return this;
  };

  connect = (sessionKey: string, encryptionKey: string, emulator: TerminalEmulator) => {
    const config = new this.module.BrowserConfig(sessionKey, encryptionKey, emulator);
    this.module.tunshell_init_client(config);
  };
}
