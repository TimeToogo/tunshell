// === TUNSHELL NODE SCRIPT ===
(async () => {
  const process = require("process");
  const https = require("https");
  const fs = require("fs");
  const os = require("os");
  const { spawn } = require("child_process");

  if (!args) {
    throw new Error(`args variable must be set`);
  }

  const getTarget = () => {
    const targets = {
      linux: {
        x64: "x86_64-unknown-linux-musl",
        x32: "i686-unknown-linux-musl",
        arm: "armv7-unknown-linux-musleabihf",
        arm64: "aarch64-unknown-linux-musl",
      },
      darwin: {
        x64: "x86_64-apple-darwin",
      },
      win32: {
        x64: "x86_64-pc-windows-msvc.exe",
        x32: "i686-pc-windows-msvc.exe",
      },
    };

    if (!targets[os.platform()]) {
      console.error(`Unsupported platform: ${os.platform()}`);
      return null;
    }

    if (!targets[os.platform()][os.arch()]) {
      console.error(`Unsupported CPU architecture: ${os.arch()}`);
      return null;
    }

    return targets[os.platform()][os.arch()];
  };

  const downloadClient = (target) => {
    const tempPath = os.tmpdir();

    if (!fs.existsSync(`${tempPath}/tunshell`)) {
      fs.mkdirSync(`${tempPath}/tunshell`, { recursive: true });
    }

    const clientPath = `${tempPath}/tunshell/client`;

    return new Promise((resolve, reject) => {
      const file = fs.createWriteStream(clientPath);

      const request = https.get(
        `https://artifacts.tunshell.com/client-${target}`,
        (response) => {
          response.pipe(file);

          response.on("end", () => {
            file.end(null);
          });

          response.on("error", reject);
        }
      );

      file.on("close", () => resolve(clientPath));
    });
  };

  const execClient = (client, args) => {
    fs.chmodSync(client, 0755);

    spawn(client, args, { stdio: "inherit" });
  };

  const target = getTarget();

  if (!target) {
    process.exit(1);
  }

  console.log("Installing client...");
  let clientPath = await downloadClient(target);

  execClient(clientPath, args);
})();
