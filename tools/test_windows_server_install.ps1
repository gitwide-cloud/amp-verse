# Synthetic files only: this never starts a server or touches a real install.
$ErrorActionPreference = 'Stop'
$previous = $env:LOCALAPPDATA
$fixture = Join-Path $previous ('AMPVerse\work\server-install-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture -Force | Out-Null
$package = Join-Path $fixture 'package'
New-Item -ItemType Directory -Path $package | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\deploy\install-windows-server.ps1') -Destination (Join-Path $package 'install-windows-server.ps1')
$names = @('amp-cast.exe','amp-engine.exe','amp-prepare.exe','LICENSE','THIRD_PARTY_NOTICES.md','show-windows-server-access.ps1','Show Server Access Code.cmd')
$entries = @()
foreach ($name in $names) {
    $path = Join-Path $package $name
    [IO.File]::WriteAllText($path, 'synthetic-fixture-only-' + $name)
    $entries += @{ name=$name; size=(Get-Item -LiteralPath $path).Length; sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() }
}
@{os='Windows';commit=('a'*40);files=$entries} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $package 'build.json')
# Include a synthetic manager and verify exact-source coupling and preservation.
$managerRoot = Join-Path $package 'manager'
New-Item -ItemType Directory -Path $managerRoot | Out-Null
$managerExe = Join-Path $managerRoot 'amp-server-manager.exe'
[IO.File]::WriteAllText($managerExe, 'synthetic-manager-not-executed')
$managerEntries = @(@{path='amp-server-manager.exe';size=(Get-Item $managerExe).Length;sha256=(Get-FileHash $managerExe).Hash.ToLowerInvariant()})
@{source_commit=('a'*40);files=$managerEntries} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $managerRoot 'build.json')
try {
    $env:LOCALAPPDATA = Join-Path $fixture 'account'
    $programs = Join-Path $env:LOCALAPPDATA 'Programs'
    $legacy = Join-Path $programs 'AMP Verse Server-legacy-test'
    New-Item -ItemType Directory -Path $legacy -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $legacy 'amp-cast.exe'), 'old-fixture')
    [IO.File]::WriteAllText((Join-Path $legacy 'settings.json'), 'retain-owner-profile-fixture')
    $settingsHash = (Get-FileHash -LiteralPath (Join-Path $legacy 'settings.json')).Hash
    & (Join-Path $package 'install-windows-server.ps1')
    if ((Get-FileHash -LiteralPath (Join-Path $legacy 'amp-cast.exe')).Hash -ne (Get-FileHash -LiteralPath (Join-Path $package 'amp-cast.exe')).Hash) { throw 'Existing server was not updated' }
    if (Test-Path -LiteralPath (Join-Path $programs 'AMP Verse Server')) { throw 'Created a duplicate default installation' }
    if ((Get-FileHash -LiteralPath (Join-Path $legacy 'settings.json')).Hash -ne $settingsHash) { throw 'Existing settings changed' }
    $installedManager = Join-Path $legacy 'manager\amp-server-manager.exe'
    if ((Get-FileHash $installedManager).Hash -ne (Get-FileHash $managerExe).Hash) { throw 'Matching manager not installed' }
    @{source_commit=('b'*40);files=$managerEntries} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $managerRoot 'build.json')
    $refused = $false
    try { & (Join-Path $package 'install-windows-server.ps1') } catch {
        if ($_.Exception.Message -notmatch 'commits differ') { throw }
        $refused = $true
    }
    if (!$refused) { throw 'Mismatched manager accepted' }
    if ((Get-FileHash $installedManager).Hash -ne (Get-FileHash $managerExe).Hash) { throw 'Refused install changed manager' }
    @{source_commit=('a'*40);files=$managerEntries} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $managerRoot 'build.json')
    [IO.File]::WriteAllText($managerExe, 'tampered')
    $refused = $false
    try { & (Join-Path $package 'install-windows-server.ps1') } catch {
        if ($_.Exception.Message -notmatch 'verification failed') { throw }
        $refused = $true
    }
    if (!$refused) { throw 'Tampered manager accepted' }
    [IO.File]::WriteAllText($managerExe, 'synthetic-manager-not-executed')
    $second = Join-Path $programs 'AMP Verse Server-other-test'
    New-Item -ItemType Directory -Path $second | Out-Null
    [IO.File]::WriteAllText((Join-Path $second 'amp-cast.exe'), 'second-fixture')
    $refused = $false
    try { & (Join-Path $package 'install-windows-server.ps1') } catch {
        if ($_.Exception.Message -notmatch 'More than one') { throw }
        $refused = $true
    }
    if (!$refused) { throw 'Ambiguous server installations were not refused' }
    $refused = $false
    try { & (Join-Path $package 'install-windows-server.ps1') -InstallDirectory ([IO.Path]::GetPathRoot($fixture)) } catch {
        if ($_.Exception.Message -notmatch 'volume root') { throw }
        $refused = $true
    }
    if (!$refused) { throw 'Volume-root installation was not refused' }
    Write-Output 'PASS: reused version-suffixed server, preserved settings, refused ambiguous copies and volume root; synthetic fixture retained.'
} finally { $env:LOCALAPPDATA = $previous }
