# Copies the CLI and ONNX Runtime beside HUSH.exe, then installs or removes
# the test-signed virtual audio driver. Exit 0 when the app can stay
# installed. Exit 1 when the driver was expected and did not install.
param(
    [switch]$Uninstall
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
    & $hush driver uninstall
    exit 0
}

& $hush models fetch
if ($LASTEXITCODE -ne 0) {
    Write-Host "DPDFNet8 was not downloaded. RNNoise still runs. Retry with: hush models fetch"
}

$package = Join-Path $resources "driver"
& $hush driver install --package $package
if ($LASTEXITCODE -ne 0) {
    Write-Host "The driver was not loaded. Enable test signing, reboot, then run:"
    Write-Host "  & `"$hush`" driver install --package `"$package`""
    exit 1
}
exit 0
