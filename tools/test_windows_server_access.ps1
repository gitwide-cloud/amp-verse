# Real Windows DPAPI recovery checks; never reads an owner's credential or opens UI.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\deploy\show-windows-server-access.ps1')
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('amp-access-code-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$plain = 'generated-fixture-' + [guid]::NewGuid().ToString('N')
$secure = ConvertTo-SecureString $plain -AsPlainText -Force
ConvertFrom-SecureString $secure | Set-Content -LiteralPath (Join-Path $fixture 'lan-control.dpapi')
$actual = Read-AMPServerAccessCode -Directory $fixture
if ([Net.NetworkCredential]::new('', $actual).Password -cne $plain) { throw 'DPAPI roundtrip failed' }
$before = (Get-FileHash (Join-Path $fixture 'lan-control.dpapi')).Hash
$actual.Dispose()
$secure.Dispose()
$missing = $false
try { Read-AMPServerAccessCode -Directory (Join-Path $fixture 'missing') | Out-Null } catch {
    $missing = $true
    if ($_.Exception.Message.Contains($plain)) { throw 'Secret in error' }
}
if (!$missing) { throw 'Missing code not rejected' }
if ((Get-FileHash (Join-Path $fixture 'lan-control.dpapi')).Hash -ne $before) { throw 'Read changed credential' }
$corrupt = Join-Path $fixture 'corrupt'
New-Item -ItemType Directory -Path $corrupt | Out-Null
'bad-encoded-synthetic-value' | Set-Content (Join-Path $corrupt 'lan-control.dpapi')
$failed = $false
try { Read-AMPServerAccessCode -Directory $corrupt | Out-Null } catch {
    $failed = $true
    if ($_.Exception.Message.Contains('bad-encoded')) { throw 'Raw credential in error' }
}
if (!$failed) { throw 'Corrupt code not rejected' }
Write-Output 'PASS: DPAPI read-only recovery, missing/corrupt errors sanitized; encrypted synthetic fixture retained.'
