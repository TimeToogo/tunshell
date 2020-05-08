const fs = require('fs');

fs.copyFileSync(process.execPath, __dirname + '/../dist/node');
