# Compile and test-sign the driver. Fails if the WDK is missing or the build fails.
$ErrorActionPreference = "Stop"
& (Join-Path $PSScriptRoot "install-wdk.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& (Join-Path $PSScriptRoot "build-driver.ps1")
exit $LASTEXITCODE
