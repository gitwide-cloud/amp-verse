# Roll back only the files changed by the canonical tested desktop installer.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BackupDirectory,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$BackupManifestSha256
)
$ErrorActionPreference = 'Stop'
function Assert-AppClosed {
    if (Get-Process -Name amp-player,amp-cast,amp-engine,amp-prepare,amp-update,amp-stem-worker -ErrorAction SilentlyContinue) {
        throw 'Close AMP Verse, Cast and analysis/update processes first. No process was stopped.'
    }
}
function Assert-NoLinks([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Linked rollback path is unsupported: $cursor"
        }
        $cursor = Split-Path $cursor -Parent
    }
}
Assert-AppClosed
$backupBase = Join-Path $env:LOCALAPPDATA 'AMPVerseInstallBackups'
Assert-NoLinks $backupBase
$updateLock = [IO.File]::Open((Join-Path $backupBase '.desktop-update.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try {
$backup = (Resolve-Path -LiteralPath $BackupDirectory).ProviderPath.TrimEnd('\')
Assert-NoLinks $backup
$manifest = Join-Path $backup 'backup-manifest.json'
if ((Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash -ne $BackupManifestSha256) { throw 'Backup manifest checksum mismatch' }
$state = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
$destination = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse')).TrimEnd('\')
if ($state.Version -ne 1 -or !([string]$state.Destination).Equals($destination,[StringComparison]::OrdinalIgnoreCase)) {
    throw 'This backup is not for the canonical AMP Verse installation on this account'
}
Assert-NoLinks $destination
if (Test-Path -LiteralPath (Join-Path $destination 'amp-player.next.exe')) { throw 'Resolve the pending app update before rollback' }
$seen = @{}
foreach ($record in $state.Files) {
    $relative = [string]$record.Relative
    if (!$relative -or [IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.([\\/]|$)|:') { throw "Unsafe rollback path: $relative" }
    $target = [IO.Path]::GetFullPath((Join-Path $destination $relative))
    if (!$target.StartsWith($destination+'\',[StringComparison]::OrdinalIgnoreCase) -or $seen.ContainsKey($target)) { throw 'Duplicate or escaping rollback target' }
    $seen[$target] = $true
    Assert-NoLinks $target
    if (Test-Path -LiteralPath $target) {
        if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Rollback target is not a file: $target" }
        $currentHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        if ($currentHash -ne $record.AfterSha256 -and !($record.Existed -and $currentHash -eq $record.BeforeSha256)) {
            throw "Installed file changed since this update; refusing to overwrite it: $target"
        }
    }
    if ($record.Existed) {
        $source = Join-Path $backup ('files\'+$relative)
        Assert-NoLinks $source
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $record.BeforeSha256) { throw "Backup file checksum mismatch: $source" }
    }
}
foreach ($record in $state.Files) {
    Assert-AppClosed
    $target = Join-Path $destination $record.Relative
    Assert-NoLinks $target
    if (Test-Path -LiteralPath $target -PathType Leaf) {
        $currentHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        if ($record.Existed -and $currentHash -eq $record.BeforeSha256) { continue }
        if ($currentHash -ne $record.AfterSha256) { throw "File changed during rollback: $target" }
    }
    if ($record.Existed) {
        New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
        $temporary = Join-Path (Split-Path $target -Parent) ('.amp-rollback-'+[Guid]::NewGuid().ToString('N')+'.tmp')
        try {
            Copy-Item -LiteralPath (Join-Path $backup ('files\'+$record.Relative)) -Destination $temporary
            if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ne $record.BeforeSha256) { throw "Restore verification failed: $target. Backup retained." }
            if (Test-Path -LiteralPath $target -PathType Leaf) { [IO.File]::Replace($temporary,$target,[NullString]::Value) }
            else { [IO.File]::Move($temporary,$target) }
        } finally {
            if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary }
        }
    } elseif (Test-Path -LiteralPath $target -PathType Leaf) { Remove-Item -LiteralPath $target }
}
'Rolled back manually' | Set-Content -LiteralPath (Join-Path $backup 'status.txt')
Write-Output 'Previous AMP Verse files restored. Settings, launcher and shortcuts were not changed. Nothing was launched.'
Write-Output "Settings snapshots remain available at $backup; restore them separately only if needed."

} finally { $updateLock.Dispose() }
