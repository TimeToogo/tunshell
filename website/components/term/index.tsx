import React, { useRef, useEffect, useState } from "react";
import { TerminalEmulator } from "../../services/wasm/tunshell_client";

export interface TerminalEmulatorProps {
  onEmulator: (emulator: TerminalEmulator) => void;
}

export const Term: React.SFC<TerminalEmulatorProps> = ({ onEmulator }) => {
  const ref = useRef<HTMLDivElement>();
  const [term, setTerm] = useState<import("xterm").Terminal>();
  const [fitAddon, setFitAddon] = useState<import("xterm-addon-fit").FitAddon>();

  useEffect(() => {
    (async () => {
      if (!ref.current || term) {
        return;
      }

      console.log(`Creating terminal...`);

      const [xterm, fit] = await Promise.all([
        import("xterm").then((i) => i),
        import("xterm-addon-fit").then((i) => i),
      ]);

      const newTerm = new xterm.Terminal({ logLevel: "debug" });
      const fitAddon = new fit.FitAddon();
      newTerm.loadAddon(fitAddon);
      newTerm.open(ref.current);

      await new Promise((r) => newTerm.writeln("Welcome to the tunshell terminal\r\n", r));

      setTerm(newTerm);
      setFitAddon(fitAddon);
    })();
  }, [ref.current]);

  useEffect(() => {
    if (!term) {
      return;
    }

    const emulator = {
      data: () =>
        new Promise<string>((resolve) => {
          const stop = term.onData((data) => {
            resolve(data);
            stop.dispose();
          });
        }),
      resize: () =>
        new Promise<Uint16Array>((resolve) => {
          const stop = term.onResize((size) => {
            resolve(new Uint16Array([size.cols, size.rows]));
            stop.dispose();
          });
        }),
      write: (data) => new Promise<void>((r) => term.write(data, r)),
      size: () => new Uint16Array([term.cols, term.rows]),
      clone: () => ({ ...emulator }),
    };

    onEmulator(emulator);
  }, [term]);

  useEffect(() => {
    if (!fitAddon || !ref.current) {
      return;
    }

    fitAddon.fit();
    window.addEventListener("resize", fitAddon.fit);

    return () => window.removeEventListener("resize", fitAddon.fit);
  }, [fitAddon, ref.current]);

  return <div style={{ width: "100%", height: "400px" }} ref={ref}></div>;
};
