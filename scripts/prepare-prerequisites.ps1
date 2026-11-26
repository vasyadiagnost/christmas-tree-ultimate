# Download the pinned, signed Microsoft package for embedding; never install it on the build PC.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $taskRoot 'windows/assets/prerequisites/vc-redist.json') -Raw | ConvertFrom-Json
$cache = Join-Path $taskRoot '.tools/prerequisites'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
$payload = Join-Path $cache $manifest.filename
if (-not (Test-Path -LiteralPath $payload)) {
    $staged = $payload + '.download'
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $manifest.url -OutFile $staged
        if ((Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -ine $manifest.sha256) { throw 'Downloaded prerequisite SHA256 mismatch.' }
        Move-Item -LiteralPath $staged -Destination $payload
    } finally {
        if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
    }
}
$file = Get-Item -LiteralPath $payload
if ($file.Length -ne $manifest.bytes -or (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash -ine $manifest.sha256) { throw 'Cached prerequisite does not match its pinned manifest.' }
$signature = Get-AuthenticodeSignature -LiteralPath $payload
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'Microsoft Authenticode signature could not be verified.' }
if ($file.VersionInfo.FileVersion -ne $manifest.version) { throw 'Prerequisite version mismatch.' }
[pscustomobject]@{Path=$file.FullName;Version=$manifest.version;MinimumVersion=$manifest.minimumVersion;SHA256=$manifest.sha256}
