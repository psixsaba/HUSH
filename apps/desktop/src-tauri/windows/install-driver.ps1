# Copies the CLI and ONNX Runtime beside HUSH.exe, then installs VB-CABLE.
# The test-signed HushVad driver is a developer option (HUSH_DEV_DRIVER=1
# or -DevDriver). Exit 10 when Windows still needs one reboot.
param(
    [switch]$Uninstall,
    [switch]$RemoveCable,
    [switch]$DevDriver
)

$ErrorActionPreference = "Continue"
$resources = Split-Path -Parent $MyInvocation.MyCommand.Path
$inst = $resources
while ($inst) {
    if ((Test-Path (Join-Path $inst "HUSH.exe")) -or (Test-Path (Join-Path $inst "hush.exe"))) {
        break
    }
    $parent = Split-Path -Parent $inst
    if (-not $parent -or $parent -eq $inst) { break }
    $inst = $parent
}

Get-ChildItem -Path $resources -Filter "onnxruntime*.dll" -ErrorAction SilentlyContinue | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $inst $_.Name) -Force
}
$cli = Join-Path $resources "hush.exe"
if (Test-Path $cli) {
    Copy-Item $cli (Join-Path $inst "hush.exe") -Force
}
$hush = Join-Path $inst "hush.exe"
if (-not (Test-Path $hush)) {
    Write-Host "hush.exe is not in the install directory."
    exit 1
}

if ($Uninstall) {
    if ($RemoveCable) {
        & $hush vbcable uninstall
    }
    if ($DevDriver -or $env:HUSH_DEV_DRIVER -eq "1") {
        & $hush driver uninstall
    }
    exit 0
}

& $hush models fetch
if ($LASTEXITCODE -ne 0) {
    Write-Host "DPDFNet8 was not downloaded. RNNoise still runs. Retry with: hush models fetch"
}

& $hush vbcable install
$code = $LASTEXITCODE
if ($DevDriver -or $env:HUSH_DEV_DRIVER -eq "1") {
    $package = Join-Path $resources "driver"
    Write-Host "Developer option: installing the test-signed HushVad driver."
    & $hush driver install --package $package
}
if ($code -eq 10) { exit 10 }
if ($code -ne 0) { exit $code }
exit 0
