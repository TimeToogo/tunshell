import React, { useRef, useEffect, useState } from "react";
import { TerminalEmulator as TerminalEmulatorInterface } from "../../services/wasm/tunshell_client";
import * as Styled from "./styled";

export interface TerminalEmulatorProps {
  fullScreen?: boolean;
  onEmulatorInitialised?: (emulator: TerminalEmulatorInterface, term: import("xterm").Terminal) => void;
  onClose?: () => void;
  children?: any
}

export const TerminalEmulator: React.SFC<TerminalEmulatorProps> = ({
  fullScreen,
  onClose = () => {},
  onEmulatorInitialised = () => {},
  children
}) => {
  const viewportRef = useRef();

  const [term, setTerm] = useState<import("xterm").Terminal>();
  const [fitAddon, setFitAddon] = useState<import("xterm-addon-fit").FitAddon>();

  useEffect(() => {
    if (!viewportRef.current || term) {
      return;
    }

    initialiseTerminal();
  }, [viewportRef.current]);

  useEffect(() => {
    if (!term) {
      return;
    }

    initialiseEmulatorWasmInterface();
  }, [term]);

  useEffect(() => {
    if (!fitAddon || !viewportRef.current) {
      return;
    }

    // Handle terminal resizing
    fitAddon.fit();
    window.addEventListener("resize", fitAddon.fit);

    return () => window.removeEventListener("resize", fitAddon.fit);
  }, [fitAddon, viewportRef.current]);

  const initialiseTerminal = async () => {
    console.log(`Creating terminal...`);

    const [xterm, fit] = await Promise.all([import("xterm").then((i) => i), import("xterm-addon-fit").then((i) => i)]);

    const initialisedTerm = new xterm.Terminal({ logLevel: "debug" });
    const fitAddon = new fit.FitAddon();
    initialisedTerm.loadAddon(fitAddon);
    initialisedTerm.open(viewportRef.current);

    await new Promise((r) => initialisedTerm.writeln("Welcome to the tunshell web terminal\r\n", r));

    setTerm(initialisedTerm);
    setFitAddon(fitAddon);

    initialisedTerm.focus();
  };

  const initialiseEmulatorWasmInterface = () => {
    const emulator: TerminalEmulatorInterface = {
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

    onEmulatorInitialised(emulator, term);
  };

  if (fullScreen) {
    return (
      <Styled.FullScreenWrapper>
        <Styled.Overlay />

        <Styled.Term>
          <Styled.Close onClick={() => onClose()}>
            <ion-icon name="close-circle-outline" />
          </Styled.Close>
          <Styled.TermViewport ref={viewportRef} />
        </Styled.Term>
        {children}
      </Styled.FullScreenWrapper>
    );
  } else {
    return <Styled.Term ref={viewportRef} />;
  }
};
