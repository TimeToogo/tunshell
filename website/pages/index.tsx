import React, { useState } from "react";
import Head from "next/head";

interface SessionKeys {
  hostKey: string;
  clientKey: string;
}

enum ClientHost {
  Unix,
  Windows,
  Browser,
}

enum TargetHost {
  Unix,
  Windows,
  Node,
}

const ClientHostScript = ({ host, sessionKey }) => {
  switch (host) {
    case ClientHost.Unix:
      return <pre>sh &lt;(curl -sSf https://lets.tunshell.com/{sessionKey}.sh)</pre>;
    case ClientHost.Windows:
      return (
        <pre>
          [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12; iex
          ((New-Object System.Net.WebClient).DownloadString('https://lets.tunshell.com/{sessionKey}.ps1'))
        </pre>
      );
    case ClientHost.Browser:
      return <pre>TODO</pre>;
  }
};

const TargetHostScript = ({ host, sessionKey }) => {
  switch (host) {
    case TargetHost.Unix:
      return <pre>curl -sSf https://lets.tunshell.com/{sessionKey}.sh | sh</pre>;
    case TargetHost.Windows:
      return <ClientHostScript host={ClientHost.Windows} sessionKey={sessionKey} />;
    case TargetHost.Node:
      return (
        <pre>{`require('https').get('https://lets.tunshell.com/${sessionKey}.js',r=>{let s="";r.setEncoding('utf8');r.on('data',(d)=>s+=d);r.on('end',()=>require('vm').runInNewContext(s,{require}))});`}</pre>
      );
  }
};

const getOptions = (enumClass: any): [string, string][] => {
  return Object.keys(enumClass)
    .filter((i) => /[^0-9]/.test(i))
    .map((i) => [enumClass[i], i]);
};

export default function Home() {
  const [creatingSession, setCreatingSession] = useState<boolean>(false);
  const [sessionKeys, setSessionKeys] = useState<SessionKeys>();

  const [clientHost, setClientHost] = useState<ClientHost>(ClientHost.Unix);
  const [targetHost, setTargetHost] = useState<TargetHost>(TargetHost.Unix);

  const createSession = async () => {
    setCreatingSession(true);
    setSessionKeys(undefined);

    try {
      const response = await fetch("https://relay.tunshell.com/api/sessions", {
        method: "POST",
      }).then((i) => i.json());

      setSessionKeys({
        hostKey: response.host_key,
        clientKey: response.client_key,
      });
    } finally {
      setCreatingSession(false);
    }
  };

  return (
    <div className="container">
      <Head>
        <title>Tunshell</title>
        <link rel="icon" href="/favicon.ico" />
      </Head>

      <main>
        <h1 className="title">Welcome to Tunshell</h1>

        <ol>
          <li>
            <button onClick={createSession} disabled={creatingSession}>
              Create a session
            </button>
          </li>
          {creatingSession && <li>Loading...</li>}
          {sessionKeys && (
            <>
              <li>
                Run this command on the <strong>target host</strong>:
                <TargetHostScript host={targetHost} sessionKey={sessionKeys.hostKey} />
                <div>
                  {getOptions(TargetHost).map(([k, v]) => (
                    <button onClick={() => setTargetHost(k as any)}>{v}</button>
                  ))}
                </div>
              </li>

              <li>
                Run this command on your <strong>local host</strong>:
                <ClientHostScript host={clientHost} sessionKey={sessionKeys.clientKey} />
                <div>
                  {getOptions(ClientHost).map(([k, v]) => (
                    <button onClick={() => setClientHost(k as any)}>{v}</button>
                  ))}
                </div>
              </li>
            </>
          )}
        </ol>
      </main>

      <style jsx>{`
        .container {
          min-height: 100vh;
          padding: 0 0.5rem;
          display: flex;
          flex-direction: column;
          justify-content: center;
          align-items: center;
        }

        main {
          padding: 5rem 0;
          flex: 1;
          display: flex;
          flex-direction: column;
          justify-content: center;
          align-items: center;
        }

        ol * {
          font-size: 16px;
        }

        ol li {
          margin-bottom: 20px;
        }
      `}</style>

      <style jsx global>{`
        html,
        body {
          padding: 0;
          margin: 0;
          font-family: -apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Oxygen, Ubuntu, Cantarell, Fira Sans,
            Droid Sans, Helvetica Neue, sans-serif;
        }

        * {
          box-sizing: border-box;
        }

        pre {
          background: #eee;
          padding: 1rem;
          width: 50vw;
          white-space: pre-wrap;
        }
      `}</style>
    </div>
  );
}
