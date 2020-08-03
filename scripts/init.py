# === TUNSHELL PYTHON3 SCRIPT ===
import platform
import tempfile
import urllib.request
import os
import sys
import subprocess

def get_target():
    targets = {
      'Linux': {
        'x86_64': 'x86_64-unknown-linux-musl',
        'arm': 'armv7-unknown-linux-musleabihf',
        'i686': 'i686-unknown-linux-musl',
      },
      'Darwin': {
        'x86_64': 'x86_64-apple-darwin',
      },
      'Windows': {
        'x86_64': 'x86_64-pc-windows-msvc',
        'i686': 'i686-pc-windows-msvc',
      },
    };

    system = platform.system()
    arch = platform.machine()

    if system not in targets:
        raise Exception(f'Unsupported platform: {system}')

    if arch not in targets[system]:
        raise Exception(f'Unsupported CPU architecture: {arch}')

    return targets[system][arch]
    
def run():
    print('Installing client...')
    target = get_target()

    with tempfile.NamedTemporaryFile() as tmp:
        r = urllib.request.urlopen(f'https://artifacts.tunshell.com/client-{target}')
        tmp.write(r.read())
        os.chmod(tmp.name, 0o755)
        subprocess.run([tmp.name] + p)

run()
