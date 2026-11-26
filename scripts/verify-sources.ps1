# Source-only checks: no application, installer, Rust test binaries or build scripts are compiled or run.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $taskRoot
$previousPath = $env:PATH
$previousCargo = $env:CARGO_HOME
$previousRustup = $env:RUSTUP_HOME
try {
    $localCargo = Join-Path $taskRoot '.tools\cargo'
    if (Test-Path -LiteralPath (Join-Path $localCargo 'bin\cargo.exe')) {
        $env:CARGO_HOME = $localCargo
        $env:RUSTUP_HOME = Join-Path $taskRoot '.tools\rustup'
        $env:PATH = (Join-Path $localCargo 'bin') + [IO.Path]::PathSeparator + $previousPath
    }
    foreach ($manifest in @('core/Cargo.toml', 'windows/Cargo.toml')) {
        & cargo fmt --manifest-path $manifest -- --check
        if ($LASTEXITCODE -ne 0) { throw "Rust formatting or parsing failed: $manifest" }
        $metadata = & cargo metadata --offline --locked --no-deps --format-version 1 --manifest-path $manifest
        if ($LASTEXITCODE -ne 0) { throw "Manifest validation failed: $manifest" }
        $null = $metadata | ConvertFrom-Json
    }

    # Every locale must contain the same keys and substitution placeholders.
    $reference = $null
    foreach ($file in Get-ChildItem -LiteralPath 'locales' -Filter '*.strings' | Sort-Object Name) {
        $table = @{}
        foreach ($line in Get-Content -LiteralPath $file.FullName -Encoding UTF8) {
            if ($line -match '^\s*"(?<key>(?:\\.|[^"\\])*)"\s*=\s*"(?<value>(?:\\.|[^"\\])*)"\s*;\s*$') {
                $key = $Matches.key
                $value = $Matches.value
                if ($table.ContainsKey($key) -or [string]::IsNullOrWhiteSpace($value)) { throw "Duplicate/empty translation: $($file.Name):$key" }
                $table[$key] = ([regex]::Matches($value, '\{[^{}]+\}') | ForEach-Object Value | Sort-Object) -join '|'
            }
        }
        if ($null -eq $reference) { $reference = $table }
        if (@(Compare-Object @($reference.Keys | Sort-Object) @($table.Keys | Sort-Object)).Count -ne 0) { throw "Translation keys differ: $($file.Name)" }
        foreach ($key in $reference.Keys) {
            if ($reference[$key] -ne $table[$key]) { throw "Translation placeholders differ: $($file.Name):$key" }
        }
    }
    Write-Output "Translations: $($reference.Count) keys in all 9 tables."

    # D3DCompile produces bytecode in memory only; no shader or executable artifact is written.
    if (-not ('ChristmasTreeSourceChecks.Shaders' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
namespace ChristmasTreeSourceChecks {
    [ComImport, Guid("8BA5FB08-5195-40E2-AC58-0D989C3A0102"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface Blob {
        [PreserveSig] IntPtr GetBufferPointer();
        [PreserveSig] UIntPtr GetBufferSize();
    }
    public static class Shaders {
        [DllImport("d3dcompiler_47.dll", CallingConvention=CallingConvention.StdCall, CharSet=CharSet.Ansi)]
        static extern int D3DCompile(byte[] data, UIntPtr length, string name, IntPtr defines, IntPtr includes,
            string entry, string profile, uint flags1, uint flags2, out Blob code, out Blob errors);
        public static void Check(string source, string entry, string profile) {
            byte[] bytes = Encoding.UTF8.GetBytes(source);
            Blob code = null, errors = null;
            try {
                int hr = D3DCompile(bytes, (UIntPtr)bytes.Length, "source-check", IntPtr.Zero, IntPtr.Zero,
                    entry, profile, 1u << 15, 0, out code, out errors);
                if (hr < 0) throw new Exception(entry + ": " + (errors == null ? hr.ToString("X8") : Marshal.PtrToStringAnsi(errors.GetBufferPointer())));
            } finally {
                if (code != null) Marshal.ReleaseComObject(code);
                if (errors != null) Marshal.ReleaseComObject(errors);
            }
        }
    }
}
'@
    }
    $source = (@('leaves.hlsl', 'shaders.hlsl', 'christmas.hlsl') | ForEach-Object { Get-Content -LiteralPath (Join-Path 'windows/src' $_) -Raw -Encoding UTF8 }) -join "`n"
    $renderer = Get-Content -LiteralPath 'windows/src/render.rs' -Raw -Encoding UTF8
    $entries = [regex]::Matches($renderer, 'compile\(s!\("(?<entry>\w+)"\),\s*s!\("(?<profile>[vp]s_4_0)"\)\)')
    if ($entries.Count -lt 14) { throw 'Renderer shader entry points were not found.' }
    foreach ($entry in $entries) {
        [ChristmasTreeSourceChecks.Shaders]::Check($source, $entry.Groups['entry'].Value, $entry.Groups['profile'].Value)
    }
    Write-Output "HLSL: $($entries.Count) entry points validated in memory."
    if (Test-Path -LiteralPath '.git') {
        & git -c core.safecrlf=false diff --check
        if ($LASTEXITCODE -ne 0) { throw 'Git whitespace check failed.' }
    }
    Write-Output 'Source checks passed. The Windows app and Rust tests were not built or run.'
} finally {
    $env:PATH = $previousPath
    $env:CARGO_HOME = $previousCargo
    $env:RUSTUP_HOME = $previousRustup
    Pop-Location
}
