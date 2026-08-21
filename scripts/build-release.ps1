param(
    [string]$Version = "1.0.1",
    [string]$ArtifactsDir = ""
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $ArtifactsDir) {
    $ArtifactsDir = Join-Path $projectRoot "artifacts"
}
$ArtifactsDir = [IO.Path]::GetFullPath($ArtifactsDir)
New-Item -ItemType Directory -Force -Path $ArtifactsDir | Out-Null

$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path -LiteralPath $cargo -PathType Leaf)) {
    throw "Cargo not found: $cargo"
}
$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
    throw "vswhere not found: $vswhere"
}
$vsInstall = & $vswhere -latest -products "*" `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath
if (-not $vsInstall) {
    throw "Visual Studio C++ Build Tools not found"
}
$devCmd = Join-Path $vsInstall "Common7\Tools\VsDevCmd.bat"
$makensis = "C:\Program Files (x86)\NSIS\makensis.exe"
if (-not (Test-Path -LiteralPath $makensis -PathType Leaf)) {
    throw "NSIS not found: $makensis"
}

Push-Location $projectRoot
try {
    & $cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt --check failed" }

    foreach ($cargoArguments in @(
        "test --all-targets",
        "clippy --all-targets -- -D warnings",
        "build --release --package cursor-zh-launcher"
    )) {
        $developerCommand = '"' + $devCmd + '" -no_logo -arch=x64 -host_arch=x64 && "' + `
            $cargo + '" ' + $cargoArguments
        & cmd.exe /d /s /c $developerCommand
        if ($LASTEXITCODE -ne 0) {
            throw "Rust release gate failed: cargo $cargoArguments"
        }
    }

    $sourceExe = Join-Path $projectRoot "target\release\cursor-zh-launcher.exe"
    $portableExe = Join-Path $ArtifactsDir "CursorZhLauncher-$Version.exe"
    Copy-Item -LiteralPath $sourceExe -Destination $portableExe -Force

    $installerPath = Join-Path $ArtifactsDir "CursorZhLauncher-$Version-Setup.exe"
    & $makensis "/INPUTCHARSET" "UTF8" "/DVERSION=$Version" "/DSOURCE_EXE=$sourceExe" `
        "/DPROJECT_ROOT=$projectRoot" "/DOUTPUT_FILE=$installerPath" `
        (Join-Path $projectRoot "installer\CursorZhLauncher.nsi")
    if ($LASTEXITCODE -ne 0) { throw "NSIS build failed" }

    foreach ($artifact in @($portableExe, $installerPath)) {
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact).Hash.ToLowerInvariant()
        $hashFile = "$artifact.sha256"
        [IO.File]::WriteAllText($hashFile, "$hash  $([IO.Path]::GetFileName($artifact))`n")
        Write-Host "artifact: $artifact"
        Write-Host "sha256:  $hash"
    }
} finally {
    Pop-Location
}
