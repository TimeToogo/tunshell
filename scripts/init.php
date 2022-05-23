// === TUNSHELL PHP SCRIPT ===

return function (array $args) { 
  echo "Installing client...\n";

  $targets = [
    'Linux' => [
      'x86_64' => 'x86_64-unknown-linux-musl',
      'i686' => 'i686-unknown-linux-musl',
      'armv7' => 'armv7-unknown-linux-musleabihf',
      'aarch64' => 'aarch64-unknown-linux-musl',
    ],
    'Darwin' => [
      'x86_64' => 'x86_64-apple-darwin',
      'arm64' => 'aarch64-apple-darwin',
    ],
    'WindowsNT' => [
      'x86_64' => 'x86_64-pc-windows-msvc.exe',
      'i686' => 'i686-pc-windows-msvc.exe',
    ]
  ];

  $os = php_uname('s');
  $arch = php_uname('m');

  if (!$targets[$os]) {
    throw new Exception("Unsupported OS: $os");
  }

  if (!$targets[$os][$arch]) {
    throw new Exception("Unsupported CPU architecture: $arch");
  }

  $target = $targets[$os][$arch];
  $clientPath = tempnam(sys_get_temp_dir(), 'tunshell_client_');
  copy("https://artifacts.tunshell.com/client-$target", $clientPath);
  chmod($clientPath, 0755);

  $args = implode(' ', $args);
  $process = proc_open("$clientPath $args", [
    0 => ['pipe', 'r'],
    1 => ['pipe', 'w'],
    2 => ['pipe', 'w'],
  ], $pipes);

  stream_set_blocking($pipes[1], 0);
  stream_set_blocking($pipes[2], 0);

  while (is_resource($pipes[1]) || is_resource($pipes[2]))
  {
      if (is_resource($pipes[1])) {
        if(feof($pipes[1]))
        {
            fclose($pipes[1]);
        }
        else
        {
            echo fgets($pipes[1], 1024);
        }
      }
    
      if (is_resource($pipes[2])) {
        if(feof($pipes[2]))
        {
            fclose($pipes[2]);
        }
        else
        {
            echo fgets($pipes[2], 1024);
        }
      }
  }
        
  proc_close($process);
};
