rem === TUNSHELL CMD SCRIPT ===

@echo OFF

reg Query "HKLM\Hardware\Description\System\CentralProcessor\0" | find /i "x86" > NUL && set OS=32BIT || set OS=64BIT

if %OS%==32BIT (
    set TARGET="i686-pc-windows-msvc"
) else if %OS%==64BIT (
  set TARGET="x86_64-pc-windows-msvc"
) else (
    echo "Unknown CPU architecture: %OS%"
    exit 1
)

if not exist "%TEMP%\tunshell" mkdir "%TEMP%\tunshell"
set CLIENT_PATH="%TEMP%\tunshell\client.exe"

echo %CLIENT_PATH%
echo "Installing client..."
powershell -Command "[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12; (New-Object Net.WebClient).DownloadFile('https://artifacts.tunshell.com/client-%TARGET%.exe', '%CLIENT_PATH%')"

set TUNSHELL_KEY=__KEY__
%CLIENT_PATH%
