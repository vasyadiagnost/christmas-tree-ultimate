[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Path)
$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$bytes = [IO.File]::ReadAllBytes($resolved)
$pe = [BitConverter]::ToInt32($bytes, 60)
if ([BitConverter]::ToUInt32($bytes, $pe) -ne 0x00004550) { throw 'Not a Windows PE file.' }
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$count = [BitConverter]::ToUInt16($bytes, $pe + 6)
$optionalSize = [BitConverter]::ToUInt16($bytes, $pe + 20)
$optional = $pe + 24
if ([BitConverter]::ToUInt16($bytes, $optional) -ne 0x20b) { throw 'Expected a PE32+ executable.' }
$sections = @()
for ($i = 0; $i -lt $count; $i++) {
    $at = $optional + $optionalSize + 40 * $i
    $sections += [pscustomobject]@{
        Start = [BitConverter]::ToUInt32($bytes, $at + 12)
        Size = [Math]::Max([BitConverter]::ToUInt32($bytes, $at + 8), [BitConverter]::ToUInt32($bytes, $at + 16))
        File = [BitConverter]::ToUInt32($bytes, $at + 20)
    }
}
function FileOffset([uint32]$rva) {
    foreach ($section in $sections) {
        if ($rva -ge $section.Start -and $rva -lt $section.Start + $section.Size) { return [int]($section.File + $rva - $section.Start) }
    }
    throw "Unmapped PE address: $rva"
}
$imports = @()
$delayed = @()
$table = [BitConverter]::ToUInt32($bytes, $optional + 120)
if ($table) {
    $at = FileOffset $table
    while ([BitConverter]::ToUInt32($bytes, $at + 12)) {
        $name = FileOffset ([BitConverter]::ToUInt32($bytes, $at + 12))
        $end = $name
        while ($bytes[$end]) { $end++ }
        $imports += [Text.Encoding]::ASCII.GetString($bytes, $name, $end - $name)
        $at += 20
    }
}
$delayTable = [BitConverter]::ToUInt32($bytes, $optional + 112 + 13 * 8)
if ($delayTable) {
    $at = FileOffset $delayTable
    while ([BitConverter]::ToUInt32($bytes, $at + 4)) {
        if (([BitConverter]::ToUInt32($bytes, $at) -band 1) -ne 1) { throw 'Unsupported delay-import address format.' }
        $name = FileOffset ([BitConverter]::ToUInt32($bytes, $at + 4))
        $end = $name
        while ($bytes[$end]) { $end++ }
        $delayed += [Text.Encoding]::ASCII.GetString($bytes, $name, $end - $name)
        $at += 32
    }
}
$dynamicCrt = @(@($imports) + @($delayed) | Where-Object { $_ -match '^(vcruntime|msvcp|concrt)\d|^(ucrtbase|msvcrt)\.dll$|^api-ms-win-crt-' })
[pscustomobject]@{Path=$resolved;Machine=$machine;Imports=$imports;DelayImports=$delayed;DynamicVisualCpp=$dynamicCrt}
