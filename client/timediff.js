const ntp = require('ntp2');

ntp.time({ server: process.argv[2] }, function (err, response) {
  console.log(
    'The network time diff is :',
    Date.now() - response.time.getTime(),
  );
});
