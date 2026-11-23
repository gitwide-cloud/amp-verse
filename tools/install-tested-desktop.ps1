# Update the one existing desktop installation; never replace its launcher/profile.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$DesktopArtifact,
    [Parameter(Mandatory=$true)][string]$EngineArtifact,
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ManifestSha256,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+$')][string]$RunId,
    [string[]]$SettingsDirectory = @()
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
    $resolved = (Resolve-Path -LiteralPath $Root).ProviderPath.TrimEnd('\')
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
        $item = Get-Item -Force -LiteralPath $file
        if ($item.PSIsContainer) { throw "Manifest entry must be a file: $relative" }
        $cursor = $file
        while ($cursor -and $cursor.Length -ge $resolved.Length) {
            if ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Linked artifact path: $relative" }
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

# This fixed native module is the only desktop artifact add-on exception.
# All supplied files were independently hashed by Verified-Files before selection.
function Get-StemLiveFiles($Files) {
    $required = @(
        'addons\stems-live\amp-stem-worker.exe',
        'addons\stems-live\onnxruntime.dll',
        'addons\stems-live\scnet.int8.onnx',
        'addons\stems-live\NOTICES.md',
        'addons\stems-live\module.json',
        'addons\stems-live\LICENSES\SCNet-MIT.txt',
        'addons\stems-live\LICENSES\ONNXRuntime-LICENSE.txt',
        'addons\stems-live\LICENSES\ONNXRuntime-ThirdPartyNotices.txt',
        'addons\stems-live\LICENSES\AMP-Verse-LICENSE.txt'
    )
    $allowed = $required + @('addons\stems-live\onnxruntime_providers_shared.dll')
    $selected = @()
    $seen = @{}
    foreach ($file in $Files) {
        $relative = ([string]$file.Relative).Replace('/','\')
        if ($relative -notmatch '^addons\\stems-live(?:\\|$)') { continue }
        if ($relative -cnotin $allowed -or $seen.ContainsKey($relative)) {
            throw "Unsupported or duplicate live-stem module file: $relative"
        }
        $seen[$relative] = $true
        $selected += [pscustomobject]@{ Source=$file.Source; Relative=$relative; Hash=$file.Hash }
    }
    if ($selected.Count) {
        foreach ($relative in $required) {
            if (!$seen.ContainsKey($relative)) { throw "Incomplete live-stem module: missing $relative" }
        }
    }
    return $selected
}

function Assert-AppClosed {
    if (Get-Process -Name amp-player,amp-cast,amp-engine,amp-prepare,amp-update,amp-stem-worker -ErrorAction SilentlyContinue) {
        throw 'Close AMP Verse, Cast and analysis/update processes first. No process was stopped.'
    }
}
function Assert-NoLinks([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Linked installation/backup/settings path is unsupported: $cursor"
            }
        }
        $cursor = Split-Path $cursor -Parent
    }
}
function Copy-Verified([string]$Source, [string]$Target, [string]$Hash) {
    New-Item -ItemType Directory -Force -Path (Split-Path $Target -Parent) | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Target -Force
    if ((Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -ne $Hash) { throw "Copied file failed verification: $Target" }
}
function Publish-Verified([string]$Source, [string]$Target, [string]$Hash) {
    $temporary = Join-Path (Split-Path $Target -Parent) ('.amp-install-'+[Guid]::NewGuid().ToString('N')+'.tmp')
    try {
        Copy-Verified $Source $temporary $Hash
        if (Test-Path -LiteralPath $Target -PathType Leaf) { [IO.File]::Replace($temporary,$Target,[NullString]::Value) }
        else { [IO.File]::Move($temporary,$Target) }
    } finally {
        if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary }
    }
}
function Is-Within([string]$Child, [string]$Parent) {
    return $Child.Equals($Parent,[StringComparison]::OrdinalIgnoreCase) -or
        $Child.StartsWith($Parent.TrimEnd('\') + '\',[StringComparison]::OrdinalIgnoreCase)
}
Assert-AppClosed
$destination = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse')).TrimEnd('\')
Assert-NoLinks $destination
if (!(Test-Path -LiteralPath (Join-Path $destination 'amp-player.exe') -PathType Leaf)) {
    throw 'The canonical AMP Verse installation must already exist. This script only updates it.'
}
if (Test-Path -LiteralPath (Join-Path $destination 'amp-player.next.exe')) {
    throw 'A pending staged app update exists. Resolve that update first so the preserved launcher cannot replace this delivery.'
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
$plan = @()
$allowedRoot = @('amp-player.exe','amp-cast.exe','amp-prepare.exe','amp-update.exe','amp-keylock.dll','amp-verse.ico','README.md','LICENSE','THIRD_PARTY_NOTICES.md')
foreach ($file in $desktopFiles) {
    $relative = $file.Relative.Replace('/','\')
    # Delivery scripts are not installed over the active launcher or existing settings.
    if ($relative -in $allowedRoot -or $relative -match '^(docs|licenses|tools|setup)\\') {
        $plan += [pscustomobject]@{ Source=$file.Source; Relative=$relative; Hash=$file.Hash }
    }
}
$plan += @(Get-StemLiveFiles $desktopFiles)
$plan += [pscustomobject]@{ Source=$engine[0].Source; Relative='addons\amp-engine.exe'; Hash=$engine[0].Hash }
foreach ($file in $engineFiles) {
    if ($file.Relative -match '^(licenses[\\/]|LICENSE$|THIRD_PARTY_NOTICES.md$)') {
        $plan += [pscustomobject]@{ Source=$file.Source; Relative=('engine-notices\'+$file.Relative.Replace('/','\')); Hash=$file.Hash }
    }
}
foreach ($name in @('ffmpeg.exe','ffprobe.exe','ffplay.exe')) {
    $runtime = Join-Path $destination $name
    if (!(Test-Path -LiteralPath $runtime -PathType Leaf)) { throw "Existing required audio runtime missing: $runtime" }
    Assert-PE $runtime
}
if (!(Test-Path -LiteralPath (Join-Path $destination 'AUDIO_TOOL_NOTICES.md') -PathType Leaf)) { throw 'Existing audio runtime notices are required' }
$profile = Join-Path $env:LOCALAPPDATA 'AMPVerse\V1Preview'
$settings = @(
    (Join-Path $env:APPDATA 'amp-verse'), (Join-Path $env:APPDATA 'AMPVerse'),
    (Join-Path $profile 'settings'), (Join-Path $profile 'AppData'), (Join-Path $profile 'Documents')
) + $SettingsDirectory
$backupBase = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'AMPVerseInstallBackups')).TrimEnd('\')
Assert-NoLinks $backupBase
$settingsPlan = @()
foreach ($directory in $settings) {
    $source = [IO.Path]::GetFullPath($directory).TrimEnd('\')
    if ($source.Length -le [IO.Path]::GetPathRoot($source).TrimEnd('\').Length -or
        (Is-Within $backupBase $source) -or (Is-Within $source $backupBase) -or
        (Is-Within $destination $source) -or (Is-Within $source $destination)) {
        throw "Settings backup would overlap installation/backups or a volume root: $source"
    }
    if ($settingsPlan | Where-Object { Is-Within $source $_.Source }) { continue }
    $settingsPlan = @($settingsPlan | Where-Object { !(Is-Within $_.Source $source) })
    Assert-NoLinks $source
    if (Test-Path -LiteralPath $source) {
        if (!(Test-Path -LiteralPath $source -PathType Container)) { throw "Settings path must be a directory: $source" }
        foreach ($entry in Get-ChildItem -LiteralPath $source -Recurse -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Linked settings entry: $($entry.FullName)" }
        }
    }
    $settingsPlan += [pscustomobject]@{ Source=$source; Existed=(Test-Path -LiteralPath $source); Backup=('settings\'+[Guid]::NewGuid().ToString('N')); Files=@() }
}
# Validate every target before creating backups or changing the installed application.
$targets = @{}
foreach ($file in $plan) {
    $target = [IO.Path]::GetFullPath((Join-Path $destination $file.Relative))
    if (!(Is-Within $target $destination) -or $target -eq $destination -or $targets.ContainsKey($target)) { throw "Duplicate or escaping update target: $target" }
    $targets[$target] = $true
    Assert-NoLinks $target
    if (Test-Path -LiteralPath $target -PathType Container) { throw "Update target is a directory: $target" }
}
New-Item -ItemType Directory -Force -Path $backupBase | Out-Null
$updateLock = [IO.File]::Open((Join-Path $backupBase '.desktop-update.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try {
$backup = Join-Path $backupBase ((Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+[Guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $backup | Out-Null
$receiptFile = Join-Path $backup 'new-desktop-receipt.json'
@{ source_commit=$SourceCommit; run_id=$RunId; manifest_sha256=$ManifestSha256; backup=$backup; installed_utc=[DateTime]::UtcNow.ToString('o') } |
    ConvertTo-Json | Set-Content -LiteralPath $receiptFile -Encoding UTF8
$receiptTarget = Join-Path $destination 'tested-desktop-receipt.json'
Assert-NoLinks $receiptTarget
if (Test-Path -LiteralPath $receiptTarget -PathType Container) { throw 'Desktop receipt target is a directory' }
$plan += [pscustomobject]@{ Source=$receiptFile; Relative='tested-desktop-receipt.json'; Hash=(Get-FileHash -LiteralPath $receiptFile -Algorithm SHA256).Hash }
$records = @()
foreach ($file in $plan) {
    $target = Join-Path $destination $file.Relative
    $staged = Join-Path $backup ('staged\'+$file.Relative)
    Copy-Verified $file.Source $staged $file.Hash
    $exists = Test-Path -LiteralPath $target -PathType Leaf
    $oldHash = $null
    if ($exists) {
        $oldHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        Copy-Verified $target (Join-Path $backup ('files\'+$file.Relative)) $oldHash
    }
    $records += [pscustomobject]@{ Relative=$file.Relative; Existed=$exists; BeforeSha256=$oldHash; AfterSha256=$file.Hash }
}
foreach ($snapshot in $settingsPlan) {
    if (!$snapshot.Existed) { continue }
    foreach ($file in Get-ChildItem -LiteralPath $snapshot.Source -Recurse -Force -File) {
        $relative = $file.FullName.Substring($snapshot.Source.Length+1)
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        Copy-Verified $file.FullName (Join-Path $backup ($snapshot.Backup+'\'+$relative)) $hash
        $snapshot.Files += [pscustomobject]@{ Relative=$relative; Sha256=$hash }
    }
}
# The active launcher's contents and its existing shortcuts are never modified.
$launcherHashes = @()
foreach ($launcher in Get-ChildItem -LiteralPath $destination -File | Where-Object { $_.Extension -in @('.vbs','.ps1','.cmd','.bat','.lnk') }) {
    $hash = (Get-FileHash -LiteralPath $launcher.FullName -Algorithm SHA256).Hash
    Copy-Verified $launcher.FullName (Join-Path $backup ('launchers\'+$launcher.Name)) $hash
    $launcherHashes += @{ Name=$launcher.Name; Sha256=$hash }
}
$state = @{ Version=1; Destination=$destination; SourceCommit=$SourceCommit; RunId=$RunId; DeliveryManifestSha256=$ManifestSha256; Files=$records; Settings=$settingsPlan; Launchers=$launcherHashes }
$stateFile = Join-Path $backup 'backup-manifest.json'
$state | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $stateFile -Encoding UTF8
$stateHash = (Get-FileHash -LiteralPath $stateFile -Algorithm SHA256).Hash
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $backup 'delivery-manifest.json')
Assert-AppClosed
$changed = @()
try {
    foreach ($record in $records) {
        Assert-AppClosed
        $target = Join-Path $destination $record.Relative
        Assert-NoLinks $target
        $changed += $record
        Publish-Verified (Join-Path $backup ('staged\'+$record.Relative)) $target $record.AfterSha256
    }
    foreach ($launcher in $launcherHashes) {
        if ((Get-FileHash -LiteralPath (Join-Path $destination $launcher.Name) -Algorithm SHA256).Hash -ne $launcher.Sha256) { throw 'Existing launcher changed during update' }
    }
    'Installed' | Set-Content -LiteralPath (Join-Path $backup 'status.txt')
} catch {
    $failure = $_
    $rollbackErrors = @()
    foreach ($record in $changed) {
        try {
            Assert-AppClosed
            $target = Join-Path $destination $record.Relative
            Assert-NoLinks $target
            if ($record.Existed) { Publish-Verified (Join-Path $backup ('files\'+$record.Relative)) $target $record.BeforeSha256 }
            elseif (Test-Path -LiteralPath $target -PathType Leaf) { Remove-Item -LiteralPath $target }
        } catch { $rollbackErrors += $_.Exception.Message }
    }
    if ($rollbackErrors.Count) {
        'Rollback needs attention' | Set-Content -LiteralPath (Join-Path $backup 'status.txt')
        throw "Update failed: $failure. Rollback needs attention: $($rollbackErrors -join '; '). Backup: $backup; manifest SHA256: $stateHash"
    }
    'Rolled back' | Set-Content -LiteralPath (Join-Path $backup 'status.txt')
    throw "Update failed and previous binaries were restored: $failure. Backup: $backup; manifest SHA256: $stateHash"
}
Write-Output "Updated the existing AMP Verse installation at $destination. No app was launched; use your existing shortcut."
Write-Output "Backup: $backup"
Write-Output "Backup manifest SHA256: $stateHash"
Write-Output 'Settings and launcher were preserved. Use restore-tested-desktop.ps1 with this backup and hash to roll back binaries; settings snapshots are retained separately.'

} finally { $updateLock.Dispose() }
