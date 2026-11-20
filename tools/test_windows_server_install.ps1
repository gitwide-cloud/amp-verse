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
