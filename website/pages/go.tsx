import React, { useState, useEffect } from "react";
import Head from "next/head";
import dynamic from "next/dynamic";
import { TerminalEmulator } from "../services/wasm/tunshell_client";
import { Term } from "../components/term";
import { Header } from "../components/header";
import { Container } from "../components/layout";
import { Wizard } from "../components/wizard";
import { Donate } from "../components/donate";

export default function Go() {
  //   const [creatingSession, setCreatingSession] = useState<boolean>(false);
  //   const [sessionKeys, setSessionKeys] = useState<SessionKeys>();
  //   const [encryptionKey, setEncryptionKey] = useState<EncryptionKey>();

  //   const [clientHost, setClientHost] = useState<ClientHost>(ClientHost.Unix);
  //   const [targetHost, setTargetHost] = useState<TargetHost>(TargetHost.Unix);

  //   const createSession = async () => {
  //     setCreatingSession(true);
  //     setSessionKeys(undefined);

  //     try {
  //       const response = await fetch("https://relay.tunshell.com/api/sessions", {
  //         method: "POST",
  //       }).then((i) => i.json());

  //       setSessionKeys(randomizeSessionKeys(response));
  //       setEncryptionKey(generateEncryptionKey());
  //     } finally {
  //       setCreatingSession(false);
  //     }
  //   };

  return (
    <div className="container">
      <Head>
        <title>Tunshell</title>
      </Head>

      <Header />
      <Wizard />

      <Donate />
    </div>
  );
}
