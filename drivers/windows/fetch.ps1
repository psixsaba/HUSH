# Clone the MIT/MS-PL Virtual Audio Driver at a fixed commit.
# Run on Windows, from this directory, after installing Git.
$ErrorActionPreference = "Stop"
$dest = Join-Path $PSScriptRoot "third_party/Virtual-Audio-Driver"
if (Test-Path $dest) {
    Write-Host "Already present: $dest"
    exit 0
}
New-Item -ItemType Directory -Force -Path (Split-Path $dest) | Out-Null
git clone --depth 1 https://github.com/VirtualDrivers/Virtual-Audio-Driver.git $dest
Write-Host "Cloned. Wire the cyclic buffer to native/ring (see README.md). Do not commit third_party/."
