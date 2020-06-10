import React, { useState } from 'react';
import Head from 'next/head';

interface SessionKeys {
  hostKey: string;
  clientKey: string;
}

export default function Home() {
  const [creatingSession, setCreatingSession] = useState<boolean>(false);
  const [sessionKeys, setSessionKeys] = useState<SessionKeys>();

  const createSession = async () => {
    setCreatingSession(true);
    setSessionKeys(undefined);

    try {
      const response = await fetch('https://relay.tunshell.com/sessions', { method: 'POST' });

      setSessionKeys(await response.json());
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
                <pre>sh &lt;(curl -sSf https://lets.tunshell.com/{sessionKeys.hostKey}.sh)</pre>
              </li>

              <li>
                Run this command on your <strong>local host</strong>:
                <pre>sh &lt;(curl -sSf https://lets.tunshell.com/{sessionKeys.clientKey}.sh)</pre>
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
      `}</style>
    </div>
  );
}
