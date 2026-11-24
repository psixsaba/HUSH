# Put the Visual Studio x64 environment on PATH and hide MinGW.
# Meson otherwise compiles WebRTC's bundled Abseil with gcc from C:\mingw64,
# and that MinGW winrt header does not build.
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    throw "vswhere.exe is not installed."
}
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) {
    $vs = & $vswhere -latest -products * -property installationPath
}
if (-not $vs) {
    throw "Visual Studio with the C++ tools was not found."
}
$bat = Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $bat)) {
    throw "vcvars64.bat is missing: $bat"
}
Write-Host "Loading $bat"

$pairs = & cmd.exe /c "`"$bat`" >nul && set"
$pathValue = $null
$include = $null
$lib = $null
$libpath = $null
foreach ($line in $pairs) {
    if ($line -match '^(.*?)=(.*)$') {
        $name = $Matches[1]
        $value = $Matches[2]
        switch ($name) {
            "PATH" { $pathValue = $value }
            "INCLUDE" { $include = $value }
            "LIB" { $lib = $value }
            "LIBPATH" { $libpath = $value }
        }
    }
}
if (-not $pathValue) {
    throw "vcvars64 did not produce a PATH."
}
$filtered = ($pathValue -split ';' | Where-Object { $_ -and ($_ -notmatch '(?i)mingw|strawberry') }) -join ';'
$env:PATH = $filtered
$env:INCLUDE = $include
$env:LIB = $lib
$env:LIBPATH = $libpath
$env:CC = "cl.exe"
$env:CXX = "cl.exe"

function Write-Env([string]$Name, [string]$Value) {
    if ($null -eq $Value) { return }
    Add-Content -Path $env:GITHUB_ENV -Value "$Name<<__HUSH_ENV__"
    Add-Content -Path $env:GITHUB_ENV -Value $Value
    Add-Content -Path $env:GITHUB_ENV -Value "__HUSH_ENV__"
}
if ($env:GITHUB_ENV) {
    Write-Env "PATH" $filtered
    Write-Env "INCLUDE" $include
    Write-Env "LIB" $lib
    Write-Env "LIBPATH" $libpath
    Write-Env "CC" "cl.exe"
    Write-Env "CXX" "cl.exe"
}
Write-Host "cl.exe: $((Get-Command cl.exe).Source)"
