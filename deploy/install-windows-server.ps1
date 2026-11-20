[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse Server'),
    [switch]$PrivateLan,
    [string]$InterfaceAlias
)

$ErrorActionPreference = 'Stop'
$destination = [IO.Path]::GetFullPath($InstallDirectory)
$manifestPath = Join-Path $PSScriptRoot 'build.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.os -ne 'Windows' -or $manifest.commit -notmatch '^[0-9a-f]{40}$') {
    throw 'Use a verified Windows standalone server package.'
}
$packageFiles = @('amp-cast.exe', 'amp-engine.exe', 'amp-prepare.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md')
foreach ($name in $packageFiles) {
    $entry = @($manifest.files | Where-Object name -ceq $name)
    $source = Join-Path $PSScriptRoot $name
    if ($entry.Count -ne 1 -or (Get-Item -LiteralPath $source).Length -ne $entry[0].size -or
        (Get-FileHash -LiteralPath $source).Hash -ne $entry[0].sha256) {
        throw "Package verification failed for $name."
    }
}
$exe = Join-Path $destination 'amp-cast.exe'
if (Get-Process amp-cast -ErrorAction SilentlyContinue | Where-Object Path -eq $exe) {
    throw 'Stop this AMP Verse Server after playback and analysis have settled, then run the installer again.'
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
