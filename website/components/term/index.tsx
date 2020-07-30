import React, { useRef, useEffect, useState } from "react";
import { TerminalEmulator } from "../../services/wasm/tunshell_client";

export interface TerminalEmulatorProps {
  onEmulator: (emulator: TerminalEmulator) => void;
}

export const Term: React.SFC<TerminalEmulatorProps> = ({ onEmulator }) => {
  const ref = useRef<HTMLDivElement>();
  const [term, setTerm] = useState<import("xterm").Terminal>();

  useEffect(() => {
    if (!ref.current || term) {
      return;
    }

    console.log(`Creating terminal...`)
    const xterm = require("xterm");
    const newTerm = new xterm.Terminal();
    newTerm.open(ref.current);
    newTerm.writeln("Welcome to the tunshell terminal");
    setTerm(newTerm);
  }, [ref.current]);

  useEffect(() => {
    if (!term) {
      return;
    }

    onEmulator({
      onData: (cb) => term.onData(cb),
      onResize: (cb) => term.onResize((size) => cb(size.cols, size.rows)),
      write: (data) => term.write(data),
      size: () => new Uint16Array([term.cols, term.rows]),
    });
  }, [term]);

  return <div style={{width: '100%', height: '400px'}} ref={ref}></div>;
};
