# Install the Windows Driver Kit and its Visual Studio toolset on a GitHub runner.
# The script fails if WindowsKernelModeDriver10.0 is still missing.
$ErrorActionPreference = "Stop"

function Test-Toolset {
    $roots = @(
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Enterprise",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Professional",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\BuildTools",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools"
    )
    foreach ($vs in $roots) {
        $toolset = Join-Path $vs "MSBuild\Microsoft\VC\v170\Platforms\x64\PlatformToolsets\WindowsKernelModeDriver10.0"
        if (Test-Path $toolset) {
            Write-Host "WDK toolset: $toolset"
            return $true
        }
    }
    return $false
}

if (Test-Toolset) {
    exit 0
}

$candidates = @(
    "https://go.microsoft.com/fwlink/?linkid=2335869",
    "https://go.microsoft.com/fwlink/?linkid=2272234",
    "https://go.microsoft.com/fwlink/?linkid=2249371"
)
$installer = Join-Path $env:TEMP "wdksetup.exe"
$installed = $false
foreach ($url in $candidates) {
    Write-Host "Downloading WDK from $url"
    try {
        Invoke-WebRequest -Uri $url -OutFile $installer -UseBasicParsing
    } catch {
        Write-Host "Download failed: $_"
        continue
    }
    $proc = Start-Process -FilePath $installer -ArgumentList "/quiet", "/norestart", "/features", "+", "/ceip", "off" -Wait -PassThru
    Write-Host "WDK setup exit code: $($proc.ExitCode)"
    if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {
        $installed = $true
        break
    }
}
if (-not $installed) {
    throw "The Windows Driver Kit installer did not succeed."
}

$vsixRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\Vsix"
if (-not (Test-Path $vsixRoot)) {
    $vsixRoot = "${env:ProgramFiles}\Windows Kits\10\Vsix"
}
$vsInstalls = @(
    "${env:ProgramFiles}\Microsoft Visual Studio\2022\Enterprise",
    "${env:ProgramFiles}\Microsoft Visual Studio\2022\Professional",
    "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community",
    "${env:ProgramFiles}\Microsoft Visual Studio\2022\BuildTools"
) | Where-Object { Test-Path "$_\Common7\IDE\VSIXInstaller.exe" }

if ((Test-Path $vsixRoot) -and $vsInstalls) {
    $vsixFiles = Get-ChildItem $vsixRoot -Filter "*.vsix" -Recurse
    foreach ($vs in $vsInstalls) {
        $vsixInstaller = "$vs\Common7\IDE\VSIXInstaller.exe"
        foreach ($vsix in $vsixFiles) {
            Write-Host "Installing $($vsix.FullName) into $vs"
            & $vsixInstaller /quiet /admin $vsix.FullName
            Write-Host "VSIXInstaller exit code: $LASTEXITCODE"
        }
    }
}

if (-not (Test-Toolset)) {
    Write-Host "Windows Kits tree:"
    Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName
    throw "WindowsKernelModeDriver10.0 is not registered with Visual Studio."
}
exit 0
