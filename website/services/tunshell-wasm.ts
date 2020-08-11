import { TerminalEmulator } from "./wasm/tunshell_client";
import { SessionKeys } from "./session";

type ClientModule = typeof import("./wasm/tunshell_client");

export class TunshellWasm {
  private module: ClientModule;
  private terminateCallback: () => void | undefined;

  constructor() {}

  init = async () => {
    let module = await import("./wasm/tunshell_client").catch((e) => {
      console.error(e);
      throw e;
    });
    this.module = module;
    return this;
  };

  connect = async (session: SessionKeys, emulator: TerminalEmulator) => {
    let terminatePromise = new Promise((resolve) => {
      this.terminateCallback = resolve;
    });

    const config = new this.module.BrowserConfig(
      session.localKey,
      session.encryptionSecret,
      emulator,
      terminatePromise
    );

    console.log(`Initialising client...`);
    await this.module.tunshell_init_client(config);
    console.log(`Client session finished...`);
  };

  terminate = () => {
    if (this.terminateCallback) {
      this.terminateCallback();
    }
  };
}
