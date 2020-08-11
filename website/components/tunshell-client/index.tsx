import dynamic from "next/dynamic";
import { TunshellWasm } from "../../services/tunshell-wasm";
import { useEffect, useState } from "react";
import { TerminalEmulator as TerminalEmulatorInterface } from "../../services/wasm/tunshell_client";
import { TerminalEmulator } from "../term";
import { SessionKeys } from "../../services/session";

interface TunshellClientProps {
  session: SessionKeys;
  onClose: () => void;
}

export const TunshellClient = dynamic<TunshellClientProps>({
  loader: async () => {
    const tunshellWasm = await new TunshellWasm().init();

    return ({ session, onClose }) => {
      const [emulatorInterface, setEmulatorInterface] = useState<TerminalEmulatorInterface>();
      const [term, setTerm] = useState<import("xterm").Terminal>();

      useEffect(() => {
        if (!emulatorInterface || !term) {
          return;
        }

        tunshellWasm
          .connect(session, emulatorInterface)
          .then(() => term.writeln("\nThe client has exited"))
          .catch(() => term.writeln("\nAn error occurred during your session"));

        return () => {
          tunshellWasm.terminate();
        };
      }, [session, emulatorInterface, term]);

      const onEmulatorInitialised = (emulatorInterface: TerminalEmulatorInterface, term: import("xterm").Terminal) => {
        setEmulatorInterface(emulatorInterface);
        setTerm(term);
      };

      return <TerminalEmulator onEmulatorInitialised={onEmulatorInitialised} onClose={onClose} fullScreen />;
    };
  },
});
