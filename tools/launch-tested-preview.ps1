# Environment belongs only to this newly launched preview process.
param([string]$MusicRoot, [switch]$AcceptanceFixture)
$ErrorActionPreference = 'Stop'
$app = $PSScriptRoot
$receipt = Get-Content -LiteralPath (Join-Path $app 'installation-receipt.json') -Raw | ConvertFrom-Json
$commit = [string]$receipt.source_commit
if ($commit -notmatch '^[0-9a-f]{40}$') { throw 'Missing delivery identity' }
if (Get-Process -Name amp-player,amp-cast -ErrorAction SilentlyContinue) {
    throw 'Close the current AMP Verse player and Cast host before starting this preview. No process was stopped.'
}
$profile = Join-Path $env:LOCALAPPDATA ('AMPVerse\TestedPreviews\' + $commit)
if ($AcceptanceFixture) {
    if (!$MusicRoot -or !(Test-Path -LiteralPath $MusicRoot -PathType Container)) { throw 'Acceptance requires an explicit fixture music folder' }
    $profile = Join-Path $profile 'acceptance'
}
foreach ($part in @('AppData','LocalAppData','settings','Documents','device','cache','data')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $profile $part) | Out-Null
}
$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = Join-Path $app 'amp-player.exe'
$start.WorkingDirectory = $app
$start.UseShellExecute = $false
if ($MusicRoot) {
    $root = (Resolve-Path -LiteralPath $MusicRoot).Path
    if ($root.Contains('"')) { throw 'Invalid music folder' }
    $argumentRoot = $root.TrimEnd('\')
    if ($argumentRoot -match '^[A-Za-z]:$') { $argumentRoot += '\.' }
    $start.Arguments = '"' + $argumentRoot + '"'
}
$values = @{
    AMP_V1_MODE='1'; AMP_CONTROL_AUTOSTART='0'; AMP_CAST_AUTOCONNECT='0';
    AMP_DEVICE_CONFIG_DIR=(Join-Path $profile 'device');
    AMP_PREPARATION_CACHE_DIR=(Join-Path $profile 'cache\preparation');
    XDG_CONFIG_HOME=(Join-Path $profile 'settings'); XDG_DATA_HOME=(Join-Path $profile 'data');
    XDG_CACHE_HOME=(Join-Path $profile 'cache'); APPDATA=(Join-Path $profile 'AppData');
    LOCALAPPDATA=(Join-Path $profile 'LocalAppData');
    AMP_VERSE_CONFIG_DIR=(Join-Path $profile 'settings');
    AMP_DOCUMENTS_DIR=(Join-Path $profile 'Documents');
    AMP_CONTROL_BIND='127.0.0.1:8790'; AMP_CAST_ADDRESS='127.0.0.1:8787';
    AMP_CAST_BIND='127.0.0.1:8787'; AMP_CAST_CONFIG_DIR=(Join-Path $profile 'settings\cast-host');
    AMP_ENGINE_EXE=(Join-Path $app 'addons\amp-engine.exe');
    AMP_LAB_TOOLS=(Join-Path $app 'tools\lab-youtube');
    AMP_STEM_PYTHON=(Join-Path $app 'addons\stems\Scripts\python.exe')
}
if ($AcceptanceFixture) { $values['USERPROFILE'] = $profile; $values['HOME'] = $profile }
foreach ($key in $values.Keys) { $start.EnvironmentVariables[$key] = $values[$key] }
$start.EnvironmentVariables.Remove('AMP_CAST_SOURCE_LOCATIONS')
$start.EnvironmentVariables.Remove('AMP_LAB_EXE')
$start.EnvironmentVariables['PATH'] = $app + ';' + $env:PATH
[Diagnostics.Process]::Start($start) | Out-Null
