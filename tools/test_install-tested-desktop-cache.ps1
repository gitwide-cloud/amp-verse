param([string]$Installer = (Join-Path $PSScriptRoot 'install-tested-desktop.ps1'))
$ErrorActionPreference = 'Stop'
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Installer,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) { Invoke-Expression $definition.Extent.Text }
$root=Join-Path ([IO.Path]::GetTempPath()) ('amp-cache-snapshot-'+[Guid]::NewGuid().ToString('N'))
$settings=Join-Path $root 'settings'
$outside=Join-Path $root 'outside'
$cache=Join-Path $settings 'prepared-stems'
New-Item -ItemType Directory -Path $settings,$outside,$cache | Out-Null
try {
    [IO.File]::WriteAllText((Join-Path $settings 'playlist.json'),'owner playlist')
    [IO.File]::WriteAllText((Join-Path $outside 'original.txt'),'outside data')
    [IO.File]::WriteAllText((Join-Path $cache 'generated.mp3'),'generated cache')
    $snapshot=Get-SettingsSnapshot $settings
    if ($snapshot.Files.Count -ne 1 -or $snapshot.ExcludedCaches.Count -ne 1) { throw 'Generated cache was included in settings backup' }
    Remove-Item -LiteralPath (Join-Path $cache 'generated.mp3')
    Remove-Item -LiteralPath $cache
    New-Item -ItemType Junction -Path $cache -Target $outside | Out-Null
    $snapshot=Get-SettingsSnapshot $settings
    if ($snapshot.Files.Count -ne 1 -or $snapshot.ExcludedCaches.Count -ne 1) { throw 'Cache junction target was traversed' }
    if ([IO.File]::ReadAllText((Join-Path $outside 'original.txt')) -ne 'outside data') { throw 'Cache target changed' }
    $linked=Join-Path $settings 'unapproved-settings-link'
    New-Item -ItemType Junction -Path $linked -Target $outside | Out-Null
    $rejected=$false
    try { Get-SettingsSnapshot $settings | Out-Null } catch { $rejected=$true }
    if (!$rejected) { throw 'Arbitrary settings link was accepted' }
    Remove-Item -LiteralPath $linked
    Write-Output 'PASS: ordinary settings retained; generated cache omitted; cache junction not traversed; other settings links rejected; target unchanged. No installation ran.'
} finally {
    # Remove junction objects first; never recursively delete through them.
    foreach ($link in @($cache,(Join-Path $settings 'unapproved-settings-link'))) {
        if (Test-Path -LiteralPath $link) {
            $item=Get-Item -LiteralPath $link -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { Remove-Item -LiteralPath $link }
        }
    }
    $resolved=[IO.Path]::GetFullPath($root)
    $temporary=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')+'\'
    if (!$resolved.StartsWith($temporary,[StringComparison]::OrdinalIgnoreCase) -or !(Split-Path $resolved -Leaf).StartsWith('amp-cache-snapshot-')) { throw 'Unexpected fixture cleanup target' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
