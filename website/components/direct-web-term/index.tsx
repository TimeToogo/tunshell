import React, { useEffect, useState } from "react";
import Head from "next/head";
import { RELAY_SERVERS } from "../../services/location";
import { SessionKeys } from "../../services/session";
import { Message } from "./styled";
import { TunshellClient } from "../tunshell-client";
import { Donate } from "../donate";
import { WebUrlService } from "../../services/direct-web-url";

const urlService = new WebUrlService();

enum State {
  Loading,
  Failed,
  Success,
  Complete,
}

export const DirectWebSession = () => {
  const [state, setState] = useState<State>(State.Loading);
  const [sessionKeys, setSessionKeys] = useState<SessionKeys>();

  useEffect(() => {
    const parsedSession = urlService.parseWebUrl(window.location.href);

    if (!parsedSession) {
      setState(State.Failed);
      return;
    }

    setSessionKeys(parsedSession);
    setState(State.Success);
  }, []);

  if (state === State.Loading) {
    return <Message>Loading...</Message>;
  }

  if (state === State.Failed) {
    return <Message>Failed to parse parameters from the anchor string, is the URL malformed?</Message>;
  }

  if (state === State.Complete) {
    return (
      <>
        <Message>Thank you for using tunshell.</Message>
        <Donate />
      </>
    );
  }

  return (
    <TunshellClient session={sessionKeys} onClose={() => setState(State.Complete)}>
      <Donate style={{ zIndex: 11 }} />
    </TunshellClient>
  );
};
