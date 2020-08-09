import dynamic from "next/dynamic";
import { TunshellWasm } from "../../services/tunshell-wasm";
import { useEffect, useState } from "react";
import { TerminalEmulator } from "../../services/wasm/tunshell_client";
import { Term } from "../term";

export const TunshellClient = dynamic({
  loader: async () => {
    const tunshellWasm = await new TunshellWasm().init();

    return ({ sessionKey, encryptionKey }: any) => {
      const [emulator, setEmulator] = useState<TerminalEmulator>();

      useEffect(() => {
        if (!emulator) {
          return;
        }

        tunshellWasm.connect(sessionKey, encryptionKey.key, emulator);

        return () => {
          tunshellWasm.terminate();
        };
      }, [sessionKey, encryptionKey, emulator]);

      return <Term onEmulator={setEmulator} />;
    };
  },
});
