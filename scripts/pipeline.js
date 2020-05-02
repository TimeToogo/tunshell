var fs = require("fs");
var crypto = require("crypto");
var net = require("net");
var inspect = require("util").inspect;
var os = require("os");
var pty = require("node-pty");

var ssh2 = require("../client/node_modules/ssh2");
var utils = ssh2.utils;

var allowedUser = Buffer.from("foo");
var allowedPassword = Buffer.from("");

var client = new ssh2.Client();

client.on("ready", () => {
  client.forwardIn("127.0.0.1", 12345, (err, bindPort) => {
    if (err) {
      console.error(err);
      throw err;
    }

    console.log("bindPort", bindPort);
  });

  client.on("tcp connection", function (info, accept, reject) {
    console.log("tcp connection", info);

    const connection = accept();

    // const sock = new net.connect({
    // host: '127.0.0.1',
    // port: 555
    // })

    var shell = pty.spawn("bash", [], {
      name: "xterm-color",
      env: process.env,
    });

    shell.onData((data) => {
      console.log("data", data);
      connection.write(data);
    });

    connection.stdin.on("data", (data) => {
      shell.write(data);
    });

    shell.on("close", () => {
      connection.end();
    });
  });
});

client.connect({
  host: "relay.debugmypipeline.com",
  username: "relay",
  password: "",
});

new ssh2.Server(
  {
    hostKeys: [fs.readFileSync(os.homedir + "/.ssh/id_rsa")],
  },
  function (client) {
    console.log("Client connected!");

    client
      .on("authentication", function (ctx) {
        var user = Buffer.from(ctx.username);
        if (
          user.length !== allowedUser.length ||
          !crypto.timingSafeEqual(user, allowedUser)
        ) {
          return ctx.reject();
        }

        ctx.accept();
      })
      .on("ready", function () {
        console.log("Client authenticated!");

        client.on("session", function (accept, reject) {
          var session = accept();
          var userPty;
          session.once("pty", function (accept, reject, info) {
            console.log("pty", info);
            userPty = info;
            accept();
          });
          session.once("shell", function (accept, reject) {
            console.log("shell");

            if (!pty) {
              reject();
            }

            var channel = accept();

            var shell = pty.spawn('bash', [], {
              name: "xterm-color",
              cols: userPty.cols,
              rows: userPty.rows,
              env: process.env,
            });

            shell.onData((data) => {
              console.log("data", data);
              channel.stdout.write(data);
            });
            channel.stdin.on("data", (data) => {
              shell.write(data);
            });

            shell.on("close", () => {
              channel.end();
            });
          });
        //   session.once("exec", function (accept, reject, info) {
        //     console.log("Client wants to execute: " + inspect(info.command));
        //     var stream = accept();
        //     stream.stderr.write("Oh no, the dreaded errors!\n");
        //     stream.write("Just kidding about the errors!\n");
        //     stream.exit(0);
        //     stream.end();
        //   });
        });
      })
      .on("end", function () {
        console.log("Client disconnected");
      });
  }
).listen(5555, "127.0.0.1", function () {
  console.log("Listening on port " + this.address().port);
});
