# Explicit next stage. This script builds binaries, and is not used by verify-sources.ps1.
[CmdletBinding()]
param([switch]$Installer, [string]$OutputDirectory = 'dist')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $taskRoot
$previousPath = $env:PATH
$previousCargo = $env:CARGO_HOME
$previousRustup = $env:RUSTUP_HOME
$previousApp = $env:CHRISTMAS_TREE_ULTIMATE_APP
$previousRedist = $env:CHRISTMAS_TREE_ULTIMATE_VC_REDIST
$previousRedistVersion = $env:CHRISTMAS_TREE_ULTIMATE_VC_VERSION
$previousRustflags = $env:RUSTFLAGS
$previousEncodedRustflags = $env:CARGO_ENCODED_RUSTFLAGS
$previousTargetDir = $env:CARGO_TARGET_DIR
$previousBuildTarget = $env:CARGO_BUILD_TARGET
try {
    if (-not (Test-Path -LiteralPath 'windows/assets/audio/santa-bells.wav' -PathType Leaf)) {
        throw 'Prepare the Santa bell asset with scripts/audio-loop-santa.py first. See RELEASING.md.'
    }
    $localCargo = Join-Path $taskRoot '.tools\cargo'
    if (Test-Path -LiteralPath (Join-Path $localCargo 'bin\cargo.exe')) {
        $env:CARGO_HOME = $localCargo
        $env:RUSTUP_HOME = Join-Path $taskRoot '.tools\rustup'
        $env:PATH = (Join-Path $localCargo 'bin') + [IO.Path]::PathSeparator + $previousPath
    }
    # Cargo searches config from the working directory, not --manifest-path.
    # Override caller flags explicitly: the app uses MSVC's default dynamic CRT;
    # the bootstrap below must start even before that runtime is installed.
    Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    Remove-Item Env:\CARGO_BUILD_TARGET -ErrorAction SilentlyContinue
    $env:CARGO_TARGET_DIR = Join-Path $taskRoot 'windows/target'
    $env:RUSTFLAGS = ' '
    & cargo fmt --manifest-path core/Cargo.toml -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Core formatting failed.' }
    & cargo fmt --manifest-path windows/Cargo.toml -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Windows formatting failed.' }
    & cargo test --locked --manifest-path core/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Core tests failed.' }
    & cargo test --locked --release --bin christmas-tree-ultimate --manifest-path windows/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Windows app tests failed.' }
    & cargo build --locked --release --bin christmas-tree-ultimate --manifest-path windows/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Windows build failed.' }
    $app = Join-Path $taskRoot 'windows\target\release\christmas-tree-ultimate.exe'
    $applicationPE = & (Join-Path $PSScriptRoot 'inspect-windows-pe.ps1') -Path $app
    if ($applicationPE.Machine -ne 0x8664 -or $applicationPE.DynamicVisualCpp.Count -eq 0) { throw 'Expected an x64 app using the checked Visual C++ prerequisite.' }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    Copy-Item -LiteralPath $app -Destination (Join-Path $OutputDirectory 'christmas-tree-ultimate.exe')
    Copy-Item -LiteralPath 'LICENSE' -Destination (Join-Path $OutputDirectory 'LICENSE')
    Copy-Item -LiteralPath 'THIRD_PARTY_NOTICES.txt' -Destination (Join-Path $OutputDirectory 'THIRD_PARTY_NOTICES.txt')
    if ($Installer) {
        $prerequisite = & (Join-Path $PSScriptRoot 'prepare-prerequisites.ps1')
        $env:CHRISTMAS_TREE_ULTIMATE_APP = $app
        $env:CHRISTMAS_TREE_ULTIMATE_VC_REDIST = $prerequisite.Path
        $env:CHRISTMAS_TREE_ULTIMATE_VC_VERSION = $prerequisite.Version
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        & cargo test --locked --release --features setup --lib --bin christmas-tree-ultimate-setup --manifest-path windows/Cargo.toml -- --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Installer tests failed.' }
        & cargo test --locked --release --features setup --lib --manifest-path windows/Cargo.toml -- --exact prerequisites::tests::official_payload_extracts_with_write_lock_and_valid_authenticode_without_installing --ignored --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Official prerequisite signature/extraction test failed.' }
        & cargo build --locked --release --features setup --bin christmas-tree-ultimate-setup --manifest-path windows/Cargo.toml
        if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
        $setup = Join-Path $taskRoot 'windows\target\release\christmas-tree-ultimate-setup.exe'
        $bootstrapPE = & (Join-Path $PSScriptRoot 'inspect-windows-pe.ps1') -Path $setup
        if ($bootstrapPE.Machine -ne 0x8664 -or $bootstrapPE.DynamicVisualCpp.Count -ne 0 -or @($bootstrapPE.Imports | Where-Object { $_ -like 'api-ms-win-crt-*' }).Count -ne 0) { throw 'Installer bootstrap has an unexpected external C runtime dependency.' }
        Copy-Item -LiteralPath $setup -Destination (Join-Path $OutputDirectory 'christmas-tree-ultimate-setup.exe')
    }
    Get-ChildItem -LiteralPath $OutputDirectory -File | Select-Object Name, Length
} finally {
    $env:PATH = $previousPath
    $env:CARGO_HOME = $previousCargo
    $env:RUSTUP_HOME = $previousRustup
    $env:CHRISTMAS_TREE_ULTIMATE_APP = $previousApp
    $env:CHRISTMAS_TREE_ULTIMATE_VC_REDIST = $previousRedist
    $env:CHRISTMAS_TREE_ULTIMATE_VC_VERSION = $previousRedistVersion
    $env:RUSTFLAGS = $previousRustflags
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustflags
    $env:CARGO_TARGET_DIR = $previousTargetDir
    $env:CARGO_BUILD_TARGET = $previousBuildTarget
    Pop-Location
}
