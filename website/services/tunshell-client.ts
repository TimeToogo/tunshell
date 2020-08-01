import { TerminalEmulator } from "./wasm/tunshell_client";

type ClientModule = typeof import("./wasm/tunshell_client");

export class TunshellClient {
  private module: ClientModule;
  private terminateCallback: () => void | undefined;

  constructor() {}

  init = async () => {
    let module = await import("./wasm/tunshell_client").catch(console.error);
    this.module = module;
    return this;
  };

  connect = (sessionKey: string, encryptionKey: string, emulator: TerminalEmulator) => {
    let terminatePromise = new Promise((resolve) => {
      this.terminateCallback = resolve;
    });

    const config = new this.module.BrowserConfig(sessionKey, encryptionKey, emulator, terminatePromise);
    this.module.tunshell_init_client(config);
  };

  terminate = () => {
    if (this.terminateCallback) {
      this.terminateCallback();
    }
  };
}
