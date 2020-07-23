// === TUNSHELL NODE SCRIPT ===
(async () => {
  const process = require("process");
  const https = require("https");
  const fs = require("fs");
  const os = require("os");
  const { spawn } = require("child_process");

  const getTarget = () => {
    const targets = {
      linux: {
        x64: "x86_64-unknown-linux-musl",
        arm: "armv7-unknown-linux-musleabihf",
        arm64: "armv7-unknown-linux-musleabihf",
        x32: "i686-unknown-linux-musl",
      },
      darwin: {
        x64: "x86_64-apple-darwin",
      },
      win32: {
        x64: "x86_64-pc-windows-msvc",
        x32: "i686-pc-windows-msvc",
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

  const execClient = (client) => {
    process.env.TUNSHELL_KEY = "__KEY__";
    fs.chmodSync(client, 0755);

    spawn(client, [], { stdio: "inherit" });
  };

  const target = getTarget();

  if (!target) {
    process.exit(1);
  }

  console.log("Installing client...");
  let clientPath = await downloadClient(target);

  execClient(clientPath);
})();
