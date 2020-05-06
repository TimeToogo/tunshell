import * as stream from 'stream';
import * as crypto from 'crypto';
import * as os from 'os';
import * as net from 'net';
import * as ssh2 from 'ssh2';
import * as pty from 'node-pty';
import * as fs from 'fs';

export interface SshConfig {
  socket: stream.Duplex;
  username: string;
  password: string;
}

export class Ssh {
  constructor(private readonly config: SshConfig) {}

  public setupSshServer = async () => {
    return new Promise((resolve, reject) => {
      const server = new ssh2.Server(
        {
          hostKeys: [this.generateOnceOffPrivateKey()],
        },
        (client) => {
          client.on('authentication', this.handleAuthentication);

          client.on('ready', () => {
            client.on('session', (accept, reject) => {
              const session = accept();
              this.createTerminalSession(session);
            });
          });

          client.on('end', () => {
            console.log('Client disconnected');
            server.close();
            resolve();
          });
        },
      );

      // Hack: grab the net.Server instance and invoke the connection listener directly.
      // This emulates an incoming connection to the server without actually listening on any port
      // Hence we can use the existing socket which has been setup
      const listener = ((server as any)._srv as net.Server).listeners('connection')[0];
      listener(this.config.socket);
    });
  };

  private generateOnceOffPrivateKey = (): Buffer => {
    const keyPair = crypto.generateKeyPairSync('rsa', { modulusLength: 4096 });
    const privateKey = keyPair.privateKey.export({ format: 'pem', type: 'pkcs1' }) as Buffer;

    return privateKey;
  };

  private handleAuthentication = (context: ssh2.AuthContext) => {
    const user = Buffer.from(context.username);
    if (
      user.length !== this.config.username.length ||
      !crypto.timingSafeEqual(user, Buffer.from(this.config.username))
    ) {
      return context.reject();
    }

    context.accept();
  };

  private createTerminalSession = (session: ssh2.Session) => {
    let userPty: ssh2.PseudoTtyInfo | undefined;

    session.once('pty', function (accept, reject, info) {
      userPty = info;
      accept();
    });

    session.once('shell', (accept, reject) => {
      if (!userPty) {
        reject();
      }

      var channel = accept();

      var shell = pty.spawn(this.getShell(), [], {
        name: 'debug-my-pipeline',
        cols: userPty.cols,
        rows: userPty.rows,
        env: process.env,
      });

      shell.onData((data) => {
        channel.stdout.write(data);
      });

      channel.stdin.pipe(shell);

      session.on('window-change', (accept, reject, info) => {
        shell.resize(info.cols, info.rows);
      });

      shell.on('exit', () => {
        channel.end();
      });
    });
  };

  private getShell = (): string => {
    const shells =
      os.platform() === 'win32'
        ? ['C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe', 'C:\\Windows\\System32\\cmd.exe']
        : [ '/bin/sh'];

    const shell = shells.find((i) => fs.statSync(i).isFile());

    if (!shell) {
      throw new Error(`Could not find shell on this platform`);
    }

    return shell;
  };

  public connectToHostSshSession = async () => {
    const client = new ssh2.Client();

    return new Promise((resolve, reject) => {
      client.connect({
        sock: this.config.socket,
        username: this.config.username,
        password: this.config.password,
      });

      client.on('ready', () => this.handleClientConnected(client, resolve, reject));
    });
  };

  private handleClientConnected = (client: ssh2.Client, resolve, reject) => {
    client.shell((error, channel) => {
      if (error) throw error;

      channel.on('close', () => {
        console.log(`Disconnected`);
        client.end();
        client.destroy();
        resolve();
      });

      console.clear();
      process.stdin.setRawMode(true);

      channel.stderr.pipe(process.stderr);
      channel.stdout.pipe(process.stdout);
      process.stdin.pipe(channel.stdin);

      const sizeTerminal = () => {
        const [width, height] = process.stdout.getWindowSize();
        channel.setWindow(height, width, height, width);
      };

      process.stdout.on('resize', sizeTerminal);
      sizeTerminal();
    });
  };
}
