import { DebugClient } from '../src/client';
import * as http from 'http';
import * as crypto from 'crypto';
import * as fs from 'fs';

let currentSession = null;

try {
  currentSession = JSON.parse(fs.readFileSync(__dirname + '/session.json').toString());
} catch (e) {}

if (currentSession) {
  new DebugClient({
    relayHost: '127.0.0.1',
    relayPort: 3001,
    verifyHostName: false,
    clientKey: currentSession.clientKey,
    directConnectStrategies: [],
  }).connect();
} else {
  (async () => {
    const result = http.request({
      method: 'POST',
      host: '127.0.0.1',
      port: 3000,
      path: '/sessions',
    });

    result.once('response', (response) => {
      response.on('data', (data) => {
        const payload = JSON.parse(data.toString());

        console.log(payload);

        fs.writeFileSync(__dirname + '/session.json', JSON.stringify(payload));

        new DebugClient({
          relayHost: '127.0.0.1',
          relayPort: 3001,
          verifyHostName: false,
          clientKey: payload.hostKey,
          directConnectStrategies: [],
        })
          .connect()
          .finally(() => {
            fs.unlinkSync(__dirname + '/session.json');
          });
      });
    });

    (result as any).end();
  })();
}
