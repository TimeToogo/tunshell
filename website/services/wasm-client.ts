import * as wasmBindings from "./wasm/tunshell_client_bg.js";

export interface TerminalEmulator {
  data: () => Promise<string>;
  resize: () => Promise<Uint16Array>;
  write: (data: string | Uint8Array) => Promise<void>;
  size: () => Uint16Array;
  clone: () => TerminalEmulator;
}

type WasmBindingModule = typeof import("./wasm/tunshell_client_bg.js");
type WasmExports = WebAssembly.Exports & {
  __wbindgen_start: () => void;
};

export interface TunshellClientModule {
  BrowserConfig: WasmBindingModule["BrowserConfig"];
  tunshell_init_client: WasmBindingModule["tunshell_init_client"];
}

let modulePromise: Promise<TunshellClientModule> | undefined;

export const loadTunshellClientModule = async (): Promise<TunshellClientModule> => {
  if (!modulePromise) {
    modulePromise = (async () => {
      const response = await fetch("/wasm/tunshell_client_bg.wasm");
      if (!response.ok) {
        throw new Error(`Failed to load the tunshell wasm module: ${response.status}`);
      }

      const source = await response.arrayBuffer();
      const { instance } = await WebAssembly.instantiate(source, {
        "./tunshell_client_bg.js": wasmBindings,
      });

      const wasm = instance.exports as WasmExports;
      wasmBindings.__wbg_set_wasm(wasm);
      wasm.__wbindgen_start();

      return {
        BrowserConfig: wasmBindings.BrowserConfig,
        tunshell_init_client: wasmBindings.tunshell_init_client,
      };
    })();
  }

  return modulePromise;
};
