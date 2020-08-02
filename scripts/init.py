# === TUNSHELL PYTHON3 SCRIPT ===
import platform
import tempfile
import requests
import os
import sys
import subprocess


def get_target():
    arch = platform.architecture()[0]
    mach = platform.machine()
    mac_ver = platform.mac_ver()
    win_ver = platform.win32_ver()
    if win_ver[0]:
        return 'x86_64-pc-windows-msvc' if arch == '64bit'
        else 'i686-pc-windows-msvc'
    elif mac_ver[0]:
        if arch == '64bit':
            return 'x86_64-apple-darwin'
        else:
            raise Exception(f'Unsupported platform: {arch} macos')
    else:
        p = {
            '64bit': 'x86_64-unknown-linux-musl',
            '32bit': 'i686-unknown-linux-musl',
            'arm': 'armv7-unknown-linux-musleabihf',
            'arm64': 'armv7-unknown-linux-musleabihf'
        }
        image = p.get(arch, None)
        if not image:
            raise Exception(f'Unsupported platform: {arch}')
        return image


def run():
    print('Installing client...')
    target = get_target()
    fp = tempfile.NamedTemporaryFile()
    url = f'https://artifacts.tunshell.com/client-{target}'
    r = requests.get(url, allow_redirects=True)
    fp.write(r.content)
    os.chmod(fp.name, 0o755)
    subprocess.run([fp.name] + p)
    fp.close()

run()
