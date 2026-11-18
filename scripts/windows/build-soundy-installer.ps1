param(
    [string]$Version = "",
    [string]$Executable = "target/release/soundy.exe",
    [string]$Output = "dist",
    [string]$Arch = "x64compatible"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "../..")

# Default to the workspace version so a release build never ships a stale installer number.
if (-not $Version) {
    $manifest = Get-Content (Join-Path $root "Cargo.toml") -Raw
    if ($manifest -match '(?m)^version = "([^"]+)"') {
        $Version = $Matches[1]
    } else {
        throw "Cargo.toml has no workspace version"
    }
}

$source = Resolve-Path (Join-Path $root $Executable)
$outputPath = Join-Path $root $Output
New-Item -ItemType Directory -Force $outputPath | Out-Null

$iscc = @(
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6/ISCC.exe")
    (Join-Path $env:LOCALAPPDATA "Programs/Inno Setup 6/ISCC.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup 6 is not installed"
}

$env:SOUNDY_VERSION = $Version.TrimStart("v")
$env:SOUNDY_EXE = $source.Path
$env:SOUNDY_DIST = (Resolve-Path $outputPath).Path
$env:SOUNDY_ARCH = $Arch
$env:SOUNDY_SETUP = "Soundy-$env:SOUNDY_VERSION-Setup"

& $iscc (Join-Path $PSScriptRoot "soundy.iss")
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed with exit code $LASTEXITCODE"
}

# The portable build is the same executable with the licence and notices beside it, so it can
# be unpacked anywhere and run without an install.
$portable = Join-Path $outputPath "Soundy-$env:SOUNDY_VERSION-Portable"
New-Item -ItemType Directory -Force $portable | Out-Null
Copy-Item $source.Path (Join-Path $portable "soundy.exe") -Force
Copy-Item (Join-Path $root "COPYING") (Join-Path $portable "LICENSE") -Force
Copy-Item (Join-Path $root "THIRD-PARTY.md") (Join-Path $portable "THIRD-PARTY.md") -Force
Copy-Item (Join-Path $root "README.md") (Join-Path $portable "README.md") -Force
Compress-Archive -Path (Join-Path $portable "*") -DestinationPath (Join-Path $outputPath "Soundy-$env:SOUNDY_VERSION-Portable.zip") -Force

Write-Output "installer: $(Join-Path $outputPath "$env:SOUNDY_SETUP.exe")"
Write-Output "portable:  $(Join-Path $outputPath "Soundy-$env:SOUNDY_VERSION-Portable.zip")"
