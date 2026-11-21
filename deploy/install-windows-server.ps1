[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse Server'),
    [switch]$PrivateLan,
    [string]$InterfaceAlias
)

$ErrorActionPreference = 'Stop'
# Historical test installs used a version suffix. Update the one existing server,
# rather than creating a second installation when no destination was supplied.
if (!$PSBoundParameters.ContainsKey('InstallDirectory')) {
    $programs = Join-Path $env:LOCALAPPDATA 'Programs'
    $known = @(Get-ChildItem -LiteralPath $programs -Directory -Filter 'AMP Verse Server*' -ErrorAction SilentlyContinue |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'amp-cast.exe') -PathType Leaf })
    if ($known.Count -gt 1) {
        throw 'More than one server installation exists. Choose the existing server with -InstallDirectory; no new installation was created.'
    }
    if ($known.Count -eq 1) { $InstallDirectory = $known[0].FullName }
}
$destination = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd('\')
if (!$destination -or $destination -eq [IO.Path]::GetPathRoot($destination).TrimEnd('\')) {
    throw 'A server installation folder is required; a volume root is not supported.'
}
$cursor = $destination
while ($cursor) {
    if ((Test-Path -LiteralPath $cursor) -and
        ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'A linked server installation path is unsupported. Existing files were not changed.'
    }
    $cursor = Split-Path $cursor -Parent
}
$manifestPath = Join-Path $PSScriptRoot 'build.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.os -ne 'Windows' -or $manifest.commit -notmatch '^[0-9a-f]{40}$') {
    throw 'Use a verified Windows standalone server package.'
}
$packageFiles = @('amp-cast.exe', 'amp-engine.exe', 'amp-prepare.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'show-windows-server-access.ps1', 'Show Server Access Code.cmd')
foreach ($name in $packageFiles) {
    $entry = @($manifest.files | Where-Object name -ceq $name)
    $source = Join-Path $PSScriptRoot $name
    if ($entry.Count -ne 1 -or (Get-Item -LiteralPath $source).Length -ne $entry[0].size -or
        (Get-FileHash -LiteralPath $source).Hash -ne $entry[0].sha256) {
        throw "Package verification failed for $name."
    }
}
foreach ($name in $packageFiles + 'build.json') {
    $target = Join-Path $destination $name
    if (Test-Path -LiteralPath $target) {
        $item = Get-Item -Force -LiteralPath $target
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'A linked or non-file server target is unsupported. Existing files were not changed.'
        }
    }
}
# Optional manager is packaged from this exact source, with a separate recursive
# manifest. Validate before touching the existing installation.
$managerFiles = @()
$managerRoot = Join-Path $PSScriptRoot 'manager'
if (Test-Path -LiteralPath $managerRoot) {
    if ((Get-Item -Force -LiteralPath $managerRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked manager package is unsupported.' }
    $managerManifest = Get-Content -LiteralPath (Join-Path $managerRoot 'build.json') -Raw | ConvertFrom-Json
    if ($managerManifest.source_commit -cne $manifest.commit) { throw 'Manager and server source commits differ.' }
    $seen = @{}
    foreach ($entry in $managerManifest.files) {
        $relative = [string]$entry.path
        if (!$relative -or [IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.?([\\/]|$)|:' -or $relative -ieq 'build.json') { throw 'Unsafe manager manifest path.' }
        $source = [IO.Path]::GetFullPath((Join-Path $managerRoot $relative))
        if ($seen.ContainsKey($source)) { throw 'Duplicate manager manifest path.' }
        $seen[$source] = $true
        $cursor = $source
        while ($cursor -and $cursor.Length -ge $managerRoot.Length) {
            if ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked manager source is unsupported.' }
            $cursor = Split-Path $cursor -Parent
        }
        $file = Get-Item -LiteralPath $source
        if ($file.PSIsContainer -or $file.Length -ne $entry.size -or
            [string]$entry.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash -LiteralPath $source).Hash -ne $entry.sha256) { throw 'Manager package verification failed.' }
        $target = Join-Path $destination ('manager\' + $relative)
        $cursor = $target
        while ($cursor -and $cursor.Length -ge $destination.Length) {
            if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -Force -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Linked manager target is unsupported.' }
            $cursor = Split-Path $cursor -Parent
        }
        $managerFiles += @{Source=$source; Target=$target; Hash=$entry.sha256}
    }
    if (!($managerManifest.files | Where-Object path -ceq 'amp-server-manager.exe')) { throw 'Manager executable is missing.' }
    if (Get-Process amp-server-manager -ErrorAction SilentlyContinue) { throw 'Exit the manager before updating it. Server playback was not changed.' }
}

$exe = Join-Path $destination 'amp-cast.exe'
if (Get-Process amp-cast -ErrorAction SilentlyContinue) {
    throw 'Close the existing AMP Verse Server after playback and analysis have settled, then rerun this installer for its existing folder. No second server was installed.'
}

$rules = @()
if ($PrivateLan) {
    $profiles = @(Get-NetConnectionProfile | Where-Object {
        $_.NetworkCategory -eq 'Private' -and (!$InterfaceAlias -or $_.InterfaceAlias -eq $InterfaceAlias)
    })
    if ($profiles.Count -ne 1) {
        throw 'Choose one active Private network interface with -InterfaceAlias.'
    }
    $profile = $profiles[0]
    $addresses = @(Get-NetIPAddress -InterfaceIndex $profile.InterfaceIndex -AddressFamily IPv4 |
        Where-Object { $_.AddressState -eq 'Preferred' -and $_.IPAddress -match '^(10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.)' })
    if ($addresses.Count -ne 1) {
        throw 'The selected interface must have one private IPv4 address.'
    }
    $rules = @(
        @{ Name = 'AMPVerse-PrivateLAN-TCP'; DisplayName = 'AMP Verse private LAN web and audio'; Protocol = 'TCP'; LocalPort = 8787; LocalAddress = $addresses[0].IPAddress },
        @{ Name = 'AMPVerse-PrivateLAN-UDP'; DisplayName = 'AMP Verse private LAN discovery'; Protocol = 'UDP'; LocalPort = 18787; LocalAddress = 'Any' }
    )
    $admin = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if (!$WhatIfPreference -and !$admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run the installer as administrator to set up private LAN access.'
    }
}

if ($PSCmdlet.ShouldProcess($destination, "Install AMP Verse Server $($manifest.commit)")) {
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    $existing = @($packageFiles + 'build.json' | Where-Object { Test-Path -LiteralPath (Join-Path $destination $_) })
    if ($existing.Count) {
        $backup = Join-Path $destination ('rollback-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $backup | Out-Null
        foreach ($name in $existing) {
            Copy-Item -LiteralPath (Join-Path $destination $name) -Destination $backup
        }
    }
    if ($managerFiles.Count) {
        $oldManager = Join-Path $destination 'manager'
        if (Test-Path -LiteralPath $oldManager) {
            if (!$backup) {
                $backup = Join-Path $destination ('rollback-' + [guid]::NewGuid().ToString('N'))
                New-Item -ItemType Directory -Path $backup | Out-Null
            }
            # Reject links anywhere in the old tree before recursive backup.
            if (Get-ChildItem -LiteralPath $oldManager -Force -Recurse | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Linked existing manager tree is unsupported.' }
            Copy-Item -LiteralPath $oldManager -Destination (Join-Path $backup 'manager') -Recurse
        }
        foreach ($file in $managerFiles) {
            New-Item -ItemType Directory -Force -Path (Split-Path $file.Target -Parent) | Out-Null
            Copy-Item -LiteralPath $file.Source -Destination $file.Target -Force
            if ((Get-FileHash -LiteralPath $file.Target).Hash -ne $file.Hash) { throw 'Installed manager hash mismatch.' }
        }
    }
    foreach ($name in $packageFiles + 'build.json') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $destination $name) -Force
    }
    foreach ($rule in $rules) {
        $scope = @{
            Profile = 'Private'; Direction = 'Inbound'; Action = 'Allow'; Enabled = 'True'
            InterfaceAlias = $profile.InterfaceAlias; RemoteAddress = 'LocalSubnet4'; Program = $exe
        }
        if (Get-NetFirewallRule -Name $rule.Name -ErrorAction SilentlyContinue) {
            $update = $rule.Clone()
            $update.Remove('DisplayName')
            Set-NetFirewallRule @update @scope
        } else {
            New-NetFirewallRule @rule @scope | Out-Null
        }
    }
}
