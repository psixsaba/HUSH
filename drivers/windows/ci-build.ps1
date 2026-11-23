# Try to compile the vendored virtual audio driver on a GitHub-hosted runner.
# windows-latest does not ship the WDK. This script installs it when it can.
# If the kit is still missing, the script exits 0 so the Rust tests stay green.
# If the kit is present, msbuild must succeed.

$ErrorActionPreference = "Continue"
$solution = Join-Path $PSScriptRoot "Virtual-Audio-Driver\VirtualAudioDriver.sln"

function Test-WdkPresent {
    return Test-Path "C:\Program Files (x86)\Windows Kits\10\bin"
}

if (-not (Test-WdkPresent)) {
    Write-Host "WDK is not installed. Trying Chocolatey windowsdriverkit11."
    if (Get-Command choco -ErrorAction SilentlyContinue) {
        choco install windowsdriverkit11 -y --no-progress
    } else {
        Write-Host "Chocolatey is not on PATH. Trying winget."
        if (Get-Command winget -ErrorAction SilentlyContinue) {
            winget install --id Microsoft.WindowsWDK.10.0.26100 --accept-package-agreements --accept-source-agreements
        }
    }
}

if (-not (Test-WdkPresent)) {
    Write-Host "WDK is still not installed. Skipping the driver compile."
    Write-Host "On a machine with the kit: msbuild VirtualAudioDriver.sln /p:Configuration=Release /p:Platform=x64"
    exit 0
}

$msbuild = Get-Command msbuild -ErrorAction SilentlyContinue
if (-not $msbuild) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $found = & $vswhere -latest -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1
        if ($found) {
            $msbuild = $found
        }
    }
}
if (-not $msbuild) {
    Write-Host "WDK is present but MSBuild is not. Skipping the driver compile."
    exit 0
}

Write-Host "Building $solution"
& $msbuild $solution /p:Configuration=Release /p:Platform=x64 /p:SignMode=Off /m
if ($LASTEXITCODE -ne 0) {
    Write-Host "Driver build failed with exit $LASTEXITCODE"
    exit $LASTEXITCODE
}
Write-Host "Driver build succeeded."
exit 0
