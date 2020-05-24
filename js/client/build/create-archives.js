const fs = require('fs');
const tar = require('tar');
const AdmZip = require('adm-zip');

const INPUT_PATH = __dirname + '/../dist/';
const OUTPUT_PATH = __dirname + '/../artifacts';

fs.mkdirSync(OUTPUT_PATH, { recursive: true });

// Create tarball
tar.c(
  {
    cwd: INPUT_PATH,
    file: OUTPUT_PATH + '/artifact.tar.gz',
    gzip: true,
    sync: true,
  },
  ['.'],
);

// Create zip
var zip = new AdmZip();
zip.addLocalFolder(INPUT_PATH);
zip.writeZip(OUTPUT_PATH + '/artifact.zip');
