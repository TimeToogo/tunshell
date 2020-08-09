// import React, { useState, useEffect } from "react";
// import Head from "next/head";
// import dynamic from "next/dynamic";
// import { TunshellClient } from "../services/tunshell-wasm";
// import { TerminalEmulator } from "../services/wasm/tunshell_client";
// import { Term } from "../components/term";

// interface SessionKeys {
//   hostKey: string;
//   clientKey: string;
// }


// const ClientHostScript = ({ host, sessionKey, encryptionKey }) => {
//   switch (host) {
//     case ClientHost.Unix:
//       return (
//         <pre>
//           sh &lt;(curl -sSf https://lets.tunshell.com/init.sh) L {sessionKey} {encryptionKey.key}
//         </pre>
//       );
//     case ClientHost.Windows:
//       return (
//         <pre>
//           [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12; &amp;
//           $([scriptblock]::Create((New-Object
//           System.Net.WebClient).DownloadString('https://lets.tunshell.com/init.ps1'))) L {sessionKey}{" "}
//           {encryptionKey.key}
//         </pre>
//       );
//     case ClientHost.Docker:
//       return (
//         <pre>
//           docker run --rm -it timetoogo/tunshell L {sessionKey} {encryptionKey.key}
//         </pre>
//       );
//     case ClientHost.Browser:
//       return (
//         <pre>
//           <InBrowserClient sessionKey={sessionKey} encryptionKey={encryptionKey} />
//         </pre>
//       );
//   }
// };

// const TargetHostScript = ({ host, sessionKey, encryptionKey }) => {
//   switch (host) {
//     case TargetHost.Unix:
//       return (
//         <pre>
//           curl -sSf https://lets.tunshell.com/init.sh | sh /dev/stdin T {sessionKey} {encryptionKey.key}
//         </pre>
//       );
//     case TargetHost.Windows:
//       return (
//         <pre>
//           [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12; &amp;
//           $([scriptblock]::Create((New-Object
//           System.Net.WebClient).DownloadString('https://lets.tunshell.com/init.ps1'))) T {sessionKey}{" "}
//           {encryptionKey.key}
//         </pre>
//       );
//     case TargetHost.Node:
//       return (
//         <pre>{`require('https').get('https://lets.tunshell.com/init.js',r=>{let s="";r.setEncoding('utf8');r.on('data',(d)=>s+=d);r.on('end',()=>require('vm').runInNewContext(s,{require,args:['T','${sessionKey}','${encryptionKey.key}']}))});`}</pre>
//       );
//     case TargetHost.Python3:
//       return (
//         <pre>{`import urllib.request;r=urllib.request.urlopen('https://lets.tunshell.com/init.py') ;exec(r.read().decode('utf-8'),{'p':['T','${sessionKey}','${encryptionKey.key}']})`}</pre>
//       );
//     case TargetHost.DotNetCore:
//       return <pre>
//         {`System.Reflection.Assembly.Load(new System.Net.WebClient().DownloadData("https://lets.tunshell.com/init.dotnet.dll")).EntryPoint.Invoke(null,new []{new string[]{"T","${sessionKey}","${encryptionKey.key}"}});`}
//       </pre>
//     case TargetHost.Java:
//       return <pre>
//         {`new URLClassLoader(new URL[]{new URL("https://lets.tunshell.com/init.jar")}).loadClass("init").getMethod("main",String[].class).invoke(null,(Object)new String[]{"T","${sessionKey}","${encryptionKey.key}"});`}
//       </pre>
//     case TargetHost.Php:
//       return <pre>
//         {`(eval(file_get_contents('https://lets.tunshell.com/init.php')))(['T','${sessionKey}','${encryptionKey.key}']);`}
//       </pre>
//   }
// };

// const getOptions = (enumClass: any): [string, string][] => {
//   return Object.keys(enumClass)
//     .filter((i) => /[^0-9]/.test(i))
//     .map((i) => [enumClass[i], i]);
// };

// const randomizeSessionKeys = (response): SessionKeys => {
//   const flip = Math.random() >= 0.5;

//   return flip
//     ? {
//         hostKey: response.peer2_key,
//         clientKey: response.peer1_key,
//       }
//     : {
//         hostKey: response.peer1_key,
//         clientKey: response.peer2_key,
//       };
// };

// export default function Home() {
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

//   return (
//     <div className="container">
//       <Head>
//         <title>Tunshell</title>
//         <link rel="icon" href="/favicon.ico" />
//       </Head>

//       <main>
//         <h1 className="title">Welcome to Tunshell</h1>

//         <ol>
//           <li>
//             <button onClick={createSession} disabled={creatingSession}>
//               Create a session
//             </button>
//           </li>
//           {creatingSession && <li>Loading...</li>}
//           {sessionKeys && encryptionKey && (
//             <>
//               <li>
//                 Run this command on the <strong>target host</strong>:
//                 <TargetHostScript host={targetHost} sessionKey={sessionKeys.hostKey} encryptionKey={encryptionKey} />
//                 <div>
//                   {getOptions(TargetHost).map(([k, v]) => (
//                     <button key={k} onClick={() => setTargetHost(k as any)}>
//                       {v}
//                     </button>
//                   ))}
//                 </div>
//               </li>

//               <li>
//                 Run this command on your <strong>local host</strong>:
//                 <ClientHostScript host={clientHost} sessionKey={sessionKeys.clientKey} encryptionKey={encryptionKey} />
//                 <div>
//                   {getOptions(ClientHost).map(([k, v]) => (
//                     <button key={k} onClick={() => setClientHost(k as any)}>
//                       {v}
//                     </button>
//                   ))}
//                 </div>
//               </li>
//             </>
//           )}
//         </ol>
//       </main>

//       <style jsx>{`
//         .container {
//           min-height: 100vh;
//           padding: 0 0.5rem;
//           display: flex;
//           flex-direction: column;
//           justify-content: center;
//           align-items: center;
//         }

//         main {
//           padding: 5rem 0;
//           flex: 1;
//           display: flex;
//           flex-direction: column;
//           justify-content: center;
//           align-items: center;
//         }

//         ol * {
//           font-size: 16px;
//         }

//         ol li {
//           margin-bottom: 20px;
//         }
//       `}</style>

//       <style jsx global>{`
//         html,
//         body {
//           padding: 0;
//           margin: 0;
//           font-family: -apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Oxygen, Ubuntu, Cantarell, Fira Sans,
//             Droid Sans, Helvetica Neue, sans-serif;
//         }

//         * {
//           box-sizing: border-box;
//         }

//         pre {
//           background: #eee;
//           padding: 1rem;
//           width: 50vw;
//           white-space: pre-wrap;
//         }
//       `}</style>
//     </div>
//   );
// }

export default () => null