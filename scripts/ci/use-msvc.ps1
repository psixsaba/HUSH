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
$meson = Get-Command meson -ErrorAction Stop
$wrapper = (Resolve-Path (Join-Path $PSScriptRoot ".")).Path
$filtered = "$wrapper;$filtered"
$env:PATH = $filtered
$env:INCLUDE = $include
$env:LIB = $lib
$env:LIBPATH = $libpath
$env:CC = "cl.exe"
$env:HUSH_MESON = $meson.Source
$llvmNm = Get-Command llvm-nm.exe -ErrorAction SilentlyContinue
if (-not $llvmNm) {
    foreach ($candidate in @(
        "${env:ProgramFiles}\LLVM\bin\llvm-nm.exe",
        "${env:ProgramFiles(x86)}\LLVM\bin\llvm-nm.exe"
    )) {
        if (Test-Path $candidate) {
            $llvmNm = Get-Item $candidate
            break
        }
    }
}
if (-not $llvmNm) {
    throw "llvm-nm.exe was not found. The WebRTC build prefixes symbols with nm."
}
$nmSource = if ($llvmNm.Source) { $llvmNm.Source } else { $llvmNm.FullName }
# Rust's process spawn looks for nm.exe. A .cmd wrapper is not found.
$nmExe = Join-Path $wrapper "nm.exe"
Copy-Item -Force $nmSource $nmExe
Write-Host "nm.exe: $nmExe copied from $nmSource"
$objcopy = Get-ChildItem "$env:USERPROFILE\.rustup\toolchains" -Recurse -Filter "rust-objcopy.exe" -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $objcopy) {
    throw "rust-objcopy.exe is missing. Install the llvm-tools rustup component."
}
Write-Host "rust-objcopy: $($objcopy.FullName)"
$cl = (Get-Command cl.exe).Source
$cxxBat = Join-Path $env:RUNNER_TEMP "hush-cl-cxx20.cmd"
# The WebRTC build script passes gcc flags to cl. Drop those, and put /std:c++20
# last so it wins over Meson's /std:c++17.
$cxxText = @"
@echo off
setlocal EnableDelayedExpansion
set "ARGS="
:loop
if "%~1"=="" goto run
if /I "%~1"=="-Wno-unused-parameter" goto next
if /I "%~1"=="-std=c++17" goto next
set ARGS=!ARGS! "%~1"
:next
shift
goto loop
:run
"$cl" !ARGS! /std:c++20
"@
$cxxText = $cxxText -replace "`n", "`r`n"
[System.IO.File]::WriteAllText($cxxBat, $cxxText)
$objNoExt = Join-Path (Split-Path $objcopy.FullName) "rust-objcopy"
Copy-Item -Force $objcopy.FullName $objNoExt
Write-Host "rust-objcopy without extension: $objNoExt"
$env:CXX = $cxxBat
$env:BINDGEN_EXTRA_CLANG_ARGS = "--target=x86_64-pc-windows-msvc"
Write-Host "meson wrapper: $wrapper"
Write-Host "real meson: $($meson.Source)"
Write-Host "cxx wrapper: $cxxBat"

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
    Write-Env "CXX" $cxxBat
    Write-Env "HUSH_MESON" $meson.Source
}
Write-Host "cl.exe: $((Get-Command cl.exe).Source)"
