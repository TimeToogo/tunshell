# === TUNSHELL PS SCRIPT ===

if ([System.Environment]::Is64BitOperatingSystem) {
  $TARGET="x86_64-pc-windows-msvc"
} else {
  $TARGET="i686-pc-windows-msvc"
}

New-Item -ItemType Directory -Force -Path "$TEMP\tunshell" > $null
$CLIENT_PATH="$TEMP\tunshell\client.exe"

Write-Host "Installing client..."
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
(New-Object Net.WebClient).DownloadFile("https://artifacts.tunshell.com/client-$TARGET.exe", "$CLIENT_PATH")

Invoke-Expression "$CLIENT_PATH $($args -join " ")"
