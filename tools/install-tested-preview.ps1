# Install an extracted, verified CI preview without changing the existing app.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$DesktopArtifact,
    [Parameter(Mandatory=$true)][string]$EngineArtifact,
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ManifestSha256,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+$')][string]$RunId
)
$ErrorActionPreference = 'Stop'
function Assert-PE([string]$File) {
    $stream = [IO.File]::OpenRead($File)
    try {
        if ($stream.Length -lt 64 -or $stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) {
            throw "Not a Windows executable: $File"
        }
        $stream.Position = 60
        $reader = New-Object IO.BinaryReader($stream)
        $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or $offset -gt ($stream.Length - 6)) { throw "Invalid PE header: $File" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 17744 -or $reader.ReadUInt16() -ne 34404) {
            throw "Not an x64 PE binary: $File"
        }
    } finally { $stream.Dispose() }
}
function Verified-Files([string]$Root, $Entries) {
    $resolved = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\')
    $result = @()
    $seen = @{}
    foreach ($entry in $Entries) {
        $relative = [string]$entry.path
        if (!$relative -or [IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.([\\/]|$)|:') {
            throw "Unsafe manifest path: $relative"
        }
        $file = [IO.Path]::GetFullPath((Join-Path $resolved $relative))
        if (!$file.StartsWith($resolved + '\', [StringComparison]::OrdinalIgnoreCase) -or $seen.ContainsKey($file)) {
            throw "Duplicate or escaping manifest path: $relative"
        }
        $seen[$file] = $true
        $item = Get-Item -LiteralPath $file
        if ($item.PSIsContainer) { throw "Manifest entry must be a file: $relative" }
        $cursor = $file
        while ($cursor -and $cursor.Length -ge $resolved.Length) {
            if ((Get-Item -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Linked artifact path: $relative" }
            $cursor = Split-Path $cursor -Parent
        }
        if ([string]$entry.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or
            (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $entry.sha256) {
            throw "Artifact checksum mismatch: $relative"
        }
        if ([IO.Path]::GetExtension($file) -in @('.exe','.dll')) { Assert-PE $file }
        $result += [pscustomobject]@{ Source=$file; Relative=$relative; Hash=[string]$entry.sha256 }
    }
    if (!$result.Count) { throw 'Empty artifact manifest' }
    return $result
}
if ((Get-FileHash -LiteralPath $Manifest -Algorithm SHA256).Hash -ne $ManifestSha256) { throw 'Delivery manifest checksum mismatch' }
$delivery = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
if ($delivery.version -ne 1 -or $delivery.source_commit -cne $SourceCommit -or
    [string]$delivery.desktop_run_id -cne $RunId -or [string]$delivery.engine_run_id -cne $RunId) {
    throw 'Desktop and engine must come from the declared commit and the same CI run'
}
$desktopFiles = @(Verified-Files $DesktopArtifact $delivery.desktop_files)
$engineFiles = @(Verified-Files $EngineArtifact $delivery.engine_files)
foreach ($name in @('amp-player.exe','amp-cast.exe','amp-prepare.exe','amp-update.exe','amp-keylock.dll','LICENSE','THIRD_PARTY_NOTICES.md')) {
    if (!($desktopFiles | Where-Object { $_.Relative -ceq $name })) { throw "Desktop artifact missing $name" }
}
$engine = @($engineFiles | Where-Object { $_.Relative -ceq 'amp-engine-windows-x86_64.exe' })
if ($engine.Count -ne 1) { throw 'Matching Windows engine missing from this delivery' }
$destination = Join-Path $env:LOCALAPPDATA ('Programs\AMP Verse Preview-' + $SourceCommit.Substring(0,12))
if (Test-Path -LiteralPath $destination) { throw "Preview already exists; retain it or choose a newer tested delivery: $destination" }
$previous = Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse'
$runtimeFiles = @()
foreach ($name in @('ffmpeg.exe','ffprobe.exe','ffplay.exe')) {
    if (!($desktopFiles | Where-Object { $_.Relative -ceq $name })) {
        $runtime = Join-Path $previous $name
        if (!(Test-Path -LiteralPath $runtime -PathType Leaf)) { throw "Required runtime missing: $runtime" }
        Assert-PE $runtime
        $runtimeFiles += Get-Item -LiteralPath $runtime
    }
}
if ($runtimeFiles.Count -and !(Test-Path -LiteralPath (Join-Path $previous 'AUDIO_TOOL_NOTICES.md') -PathType Leaf)) {
    throw 'Existing audio runtime notices are required when reusing its FFmpeg tools'
}
New-Item -ItemType Directory -Path $destination | Out-Null
foreach ($file in $desktopFiles) {
    $target = Join-Path $destination $file.Relative
    New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
    Copy-Item -LiteralPath $file.Source -Destination $target
    if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $file.Hash) { throw "Installed artifact failed verification: $target" }
}
$addons = Join-Path $destination 'addons'
New-Item -ItemType Directory -Force -Path $addons | Out-Null
Copy-Item -LiteralPath $engine[0].Source -Destination (Join-Path $addons 'amp-engine.exe')
if ((Get-FileHash -LiteralPath (Join-Path $addons 'amp-engine.exe') -Algorithm SHA256).Hash -ne $engine[0].Hash) { throw 'Installed engine failed verification' }
foreach ($file in $engineFiles) {
    if ($file.Relative -match '^(licenses[\\/]|LICENSE$|THIRD_PARTY_NOTICES.md$)') {
        $target = Join-Path $destination ('engine-notices\' + $file.Relative)
        New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
        Copy-Item -LiteralPath $file.Source -Destination $target
    }
}
$runtimeReceipt = @()
foreach ($file in $runtimeFiles) {
    $target = Join-Path $destination $file.Name
    Copy-Item -LiteralPath $file.FullName -Destination $target
    $runtimeReceipt += @{ name=$file.Name; sha256=(Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash; source=$file.FullName }
}
if ($runtimeFiles.Count) {
    foreach ($name in @('AUDIO_TOOL_NOTICES.md','licenses')) {
        $source = Join-Path $previous $name
        if (Test-Path -LiteralPath $source) {
            $notices = Join-Path $destination 'runtime-notices'
            New-Item -ItemType Directory -Force -Path $notices | Out-Null
            Copy-Item -LiteralPath $source -Destination $notices -Recurse
        }
    }
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'launch-tested-preview.ps1') -Destination $destination
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $destination 'delivery-manifest.json')
@{ source_commit=$SourceCommit; run_id=$RunId; manifest_sha256=$ManifestSha256; runtime=$runtimeReceipt } |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'installation-receipt.json') -Encoding UTF8
$shortName = 'AMP Verse Preview ' + $SourceCommit.Substring(0,12)
$shell = New-Object -ComObject WScript.Shell
foreach ($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))) {
    $link = $shell.CreateShortcut((Join-Path $folder ($shortName + '.lnk')))
    $link.TargetPath = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $link.Arguments = '-NoProfile -File "' + (Join-Path $destination 'launch-tested-preview.ps1') + '"'
    $link.WorkingDirectory = $destination
    if (Test-Path -LiteralPath (Join-Path $destination 'amp-verse.ico')) { $link.IconLocation = (Join-Path $destination 'amp-verse.ico') + ',0' }
    $link.Description = 'Tested preview; close the current AMP Verse before launching. Original installation retained.'
    $link.Save()
}
Write-Output "Installed $shortName at $destination. Nothing was launched. Close AMP Verse before opening the new shortcut."
Write-Output 'To roll back, close this preview and open your original AMP Verse shortcut. Your original installation and settings were retained.'
