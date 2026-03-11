import { loadTunshellClientModule, TerminalEmulator, TunshellClientModule } from "./wasm-client";
import { SessionKeys } from "./session";

export class TunshellWasm {
  private module: TunshellClientModule;
  private terminateCallback?: () => void;

  constructor() {}

  init = async () => {
    let module = await loadTunshellClientModule().catch((e) => {
      console.error(e);
      throw e;
    });
    this.module = module;
    return this;
  };

  connect = async (session: SessionKeys, emulator: TerminalEmulator) => {
    let terminatePromise = new Promise<void>((resolve) => {
      this.terminateCallback = () => resolve();
    });

    const config = new this.module.BrowserConfig(
      session.localKey,
      session.encryptionSecret,
      session.relayServer.domain,
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
