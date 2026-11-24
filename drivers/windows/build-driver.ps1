# Compile, test-sign, and stage the x64 virtual audio driver.
# Requires the WDK toolset. install-wdk.ps1 is the GitHub Actions setup.
$ErrorActionPreference = "Stop"

$driverRoot = Join-Path $PSScriptRoot "Virtual-Audio-Driver"
$solution = Join-Path $driverRoot "VirtualAudioDriver.sln"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$stage = Join-Path $repoRoot "dist\driver"
$zip = Join-Path $repoRoot "dist\HUSH-driver-x64.zip"
$password = "hush-test"

function Find-MsBuild {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe is not installed."
    }
    $found = & $vswhere -latest -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1
    if (-not $found) {
        throw "MSBuild was not found."
    }
    return $found
}

function Find-SdkTool([string]$Name) {
    $roots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "${env:ProgramFiles}\Windows Kits\10\bin"
    )
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        $hit = Get-ChildItem $root -Recurse -Filter $Name -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($hit) { return $hit.FullName }
    }
    throw "Could not find $Name under the Windows Kits bin directory."
}

$msbuild = Find-MsBuild
Write-Host "MSBuild: $msbuild"
Write-Host "Building $solution"
& $msbuild $solution /m /v:minimal /p:Configuration=Release /p:Platform=x64 /p:SignMode=Off /p:SpectreMitigation=false
if ($LASTEXITCODE -ne 0) {
    Write-Host "Solution build exited $LASTEXITCODE. Looking for a .sys anyway."
}

$sys = Get-ChildItem $driverRoot -Recurse -Filter "VirtualAudioDriver.sys" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' -and $_.FullName -match '\\Release\\' } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
$inf = Get-ChildItem $driverRoot -Recurse -Filter "VirtualAudioDriver.inf" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' -and $_.FullName -match '\\Release\\' } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $sys -or -not $inf) {
    throw "The driver build did not produce VirtualAudioDriver.sys and VirtualAudioDriver.inf."
}
Write-Host "sys: $($sys.FullName)"
Write-Host "inf: $($inf.FullName)"

New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $sys.FullName (Join-Path $stage "virtualaudiodriver.sys") -Force
Copy-Item $inf.FullName (Join-Path $stage "VirtualAudioDriver.inf") -Force

$cer = Join-Path $stage "HushTest.cer"
$pfx = Join-Path $stage "HushTest.pfx"
$secure = ConvertTo-SecureString -String $password -Force -AsPlainText
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=HUSH Test" -CertStoreLocation Cert:\CurrentUser\My -HashAlgorithm SHA256 -KeyExportPolicy Exportable -NotAfter (Get-Date).AddYears(5)
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $secure | Out-Null
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
Set-Content -Path (Join-Path $stage "TEST_CERT_PASSWORD.txt") -Value $password -Encoding ascii

$signtool = Find-SdkTool "signtool.exe"
$inf2cat = Find-SdkTool "inf2cat.exe"
Write-Host "signtool: $signtool"
& $signtool sign /fd SHA256 /f $pfx /p $password (Join-Path $stage "virtualaudiodriver.sys")
if ($LASTEXITCODE -ne 0) { throw "signtool failed on the .sys" }

$osSets = @(
    "10_NI_X64,10_VB_X64,Server2022_X64",
    "10_X64,10_NI_X64",
    "10_X64"
)
$catalogOk = $false
foreach ($os in $osSets) {
    Write-Host "inf2cat /os:$os"
    & $inf2cat /driver:$stage /os:$os
    if ($LASTEXITCODE -eq 0) {
        $catalogOk = $true
        break
    }
}
if (-not $catalogOk) { throw "inf2cat did not produce a catalog." }

$cat = Get-ChildItem $stage -Filter "*.cat" | Select-Object -First 1
if (-not $cat) { throw "inf2cat reported success but no .cat was written." }
& $signtool sign /fd SHA256 /f $pfx /p $password $cat.FullName
if ($LASTEXITCODE -ne 0) { throw "signtool failed on the catalog" }

if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Write-Host "Driver package: $zip"
Get-ChildItem $stage | Select-Object Name, Length
