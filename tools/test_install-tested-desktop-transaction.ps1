# Retained synthetic fixtures only. No binary execution, process mocking or cleanup deletion.
param(
    [string]$Installer = (Join-Path $PSScriptRoot 'install-tested-desktop.ps1'),
    [string]$Rollback = (Join-Path $PSScriptRoot 'restore-tested-desktop.ps1')
)
$ErrorActionPreference='Stop'
if (Get-Process -Name amp-player,amp-cast,amp-engine,amp-prepare,amp-update,amp-stem-worker -ErrorAction SilentlyContinue) {
    throw 'Transaction fixture cannot run while AMP processes are open; no process was stopped and no fixture was created.'
}
function Check([bool]$Condition,[string]$Message) { if (!$Condition) { throw $Message } }
function Must-Reject([scriptblock]$Action) {
    $rejected=$false
    try { & $Action | Out-Null } catch { $rejected=$true }
    Check $rejected 'Invalid transaction was accepted'
}
function Write-Fixture([string]$Path,[byte]$Marker) {
    New-Item -ItemType Directory -Force -Path (Split-Path $Path -Parent) | Out-Null
    $pe=New-Object byte[] 128
    $pe[0]=77; $pe[1]=90; $pe[60]=64; $pe[64]=80; $pe[65]=69; $pe[68]=100; $pe[69]=134; $pe[127]=$Marker
    [IO.File]::WriteAllBytes($Path,$pe)
}
function File-Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
$root=Join-Path ([IO.Path]::GetTempPath()) ('amp-canonical-transaction-'+[Guid]::NewGuid().ToString('N'))
$oldLocal=$env:LOCALAPPDATA; $oldRoaming=$env:APPDATA
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $env:LOCALAPPDATA=Join-Path $root 'local'; $env:APPDATA=Join-Path $root 'roaming'
    $destination=Join-Path $env:LOCALAPPDATA 'Programs\AMP Verse'
    $desktop=Join-Path $root 'desktop'; $engine=Join-Path $root 'engine'
    foreach ($name in @('amp-player.exe','amp-cast.exe','amp-prepare.exe','amp-update.exe','amp-keylock.dll')) {
        Write-Fixture (Join-Path $destination $name) 0
        Write-Fixture (Join-Path $desktop $name) 1
    }
    Write-Fixture (Join-Path $destination 'addons\amp-engine.exe') 0
    Write-Fixture (Join-Path $engine 'amp-engine-windows-x86_64.exe') 1
    foreach ($name in @('ffmpeg.exe','ffprobe.exe','ffplay.exe')) { Write-Fixture (Join-Path $destination $name) 0 }
    foreach ($name in @('LICENSE','THIRD_PARTY_NOTICES.md')) { [IO.File]::WriteAllText((Join-Path $desktop $name),'new notice') }
    [IO.File]::WriteAllText((Join-Path $destination 'AUDIO_TOOL_NOTICES.md'),'existing runtime notice')
    $launcher=Join-Path $destination 'Launch AMP Verse.vbs'
    [IO.File]::WriteAllText($launcher,'preserve active launcher')
    [IO.File]::WriteAllText((Join-Path $desktop 'Launch AMP Verse.vbs'),'artifact launcher must not replace active launcher')
    $settings=Join-Path $env:LOCALAPPDATA 'AMPVerse\V1Preview\settings'
    $custom=Join-Path $root 'custom-settings'
    foreach ($folder in @($settings,$custom)) {
        New-Item -ItemType Directory -Force -Path $folder | Out-Null
        [IO.File]::WriteAllText((Join-Path $folder 'saved.txt'),'keep settings')
    }
    $moduleRelative='addons\stems-live'
    $moduleFiles=@('amp-stem-worker.exe','onnxruntime.dll','scnet.int8.onnx','NOTICES.md','module.json','LICENSES\SCNet-MIT.txt','LICENSES\ONNXRuntime-LICENSE.txt','LICENSES\ONNXRuntime-ThirdPartyNotices.txt','LICENSES\AMP-Verse-LICENSE.txt')
    foreach ($name in $moduleFiles) {
        $target=Join-Path $desktop ($moduleRelative+'\'+$name)
        New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
        if ([IO.Path]::GetExtension($target) -in @('.exe','.dll')) { Write-Fixture $target 1 }
        else { [IO.File]::WriteAllText($target,'verified native module fixture') }
    }
    Write-Fixture (Join-Path $destination ($moduleRelative+'\amp-stem-worker.exe')) 0
    $oldStemWorker=File-Hash (Join-Path $destination ($moduleRelative+'\amp-stem-worker.exe'))
    $ownerAddon=Join-Path $destination 'addons\owner-addon\keep.txt'
    New-Item -ItemType Directory -Force -Path (Split-Path $ownerAddon -Parent) | Out-Null
    [IO.File]::WriteAllText($ownerAddon,'owner-created add-on is never replaced')
    $ownerAddonHash=File-Hash $ownerAddon
    $oldPlayer=File-Hash (Join-Path $destination 'amp-player.exe')
    $oldEngine=File-Hash (Join-Path $destination 'addons\amp-engine.exe')
    $oldLauncher=File-Hash $launcher
    $oldSettings=File-Hash (Join-Path $settings 'saved.txt')
    $commit='a'*40; $run='12345'; $manifest=Join-Path $root 'delivery.json'
    $desktopEntries=@(Get-ChildItem -LiteralPath $desktop -File -Recurse | ForEach-Object { @{ path=$_.FullName.Substring($desktop.Length+1); sha256=(File-Hash $_.FullName) } })
    $engineEntries=@(Get-ChildItem -LiteralPath $engine -File | ForEach-Object { @{ path=$_.Name; sha256=(File-Hash $_.FullName) } })
    @{ version=1; source_commit=$commit; desktop_run_id=$run; engine_run_id=$run; desktop_files=$desktopEntries; engine_files=$engineEntries } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifest -Encoding UTF8
    $arguments=@{ DesktopArtifact=$desktop; EngineArtifact=$engine; Manifest=$manifest; ManifestSha256=(File-Hash $manifest); SourceCommit=$commit; RunId=$run; SettingsDirectory=@($custom) }
    $badArguments=$arguments.Clone(); $badArguments.ManifestSha256='0'*64
    Must-Reject { & $Installer @badArguments }
    Check (!(Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'AMPVerseInstallBackups'))) 'Bad manifest mutated backup state'
    Check ((File-Hash (Join-Path $destination 'amp-player.exe')) -eq $oldPlayer) 'Bad manifest changed player'
    & $Installer @arguments | Out-Null
    Check ((File-Hash (Join-Path $destination 'amp-player.exe')) -eq (File-Hash (Join-Path $desktop 'amp-player.exe'))) 'Player was not updated'
    foreach ($name in $moduleFiles) { Check ((File-Hash (Join-Path $destination ($moduleRelative+'\'+$name))) -eq (File-Hash (Join-Path $desktop ($moduleRelative+'\'+$name)))) "Native module file was not installed: $name" }
    Check ((File-Hash $ownerAddon) -eq $ownerAddonHash) 'Unrelated add-on was changed'
    Check ((File-Hash $launcher) -eq $oldLauncher) 'Launcher was changed'
    Check ((File-Hash (Join-Path $settings 'saved.txt')) -eq $oldSettings) 'Active settings were changed'
    Check (!(Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'Programs') -Directory | Where-Object Name -ne 'AMP Verse')) 'Extra installation was created'
    $receipt=Get-Content -LiteralPath (Join-Path $destination 'tested-desktop-receipt.json') -Raw | ConvertFrom-Json
    Check ($receipt.source_commit -eq $commit -and $receipt.run_id -eq $run) 'Wrong receipt provenance'
    $backup=[string]$receipt.backup
    $backupManifest=Join-Path $backup 'backup-manifest.json'; $backupHash=File-Hash $backupManifest
    $state=Get-Content -LiteralPath $backupManifest -Raw | ConvertFrom-Json
    Check (@($state.Settings | Where-Object { $_.Source -eq $custom -and $_.Existed }).Count -eq 1) 'Custom settings not backed up'
    foreach ($snapshot in $state.Settings | Where-Object Existed) {
        foreach ($file in $snapshot.Files) { Check ((File-Hash (Join-Path $backup ($snapshot.Backup+'\'+$file.Relative))) -eq $file.Sha256) 'Settings backup checksum failed' }
    }
    Must-Reject { & $Rollback -BackupDirectory $backup -BackupManifestSha256 ('0'*64) }
    Write-Fixture (Join-Path $destination 'amp-cast.exe') 2
    Must-Reject { & $Rollback -BackupDirectory $backup -BackupManifestSha256 $backupHash }
    Copy-Item -LiteralPath (Join-Path $desktop 'amp-cast.exe') -Destination (Join-Path $destination 'amp-cast.exe') -Force
    & $Rollback -BackupDirectory $backup -BackupManifestSha256 $backupHash | Out-Null
    & $Rollback -BackupDirectory $backup -BackupManifestSha256 $backupHash | Out-Null
    Check ((File-Hash (Join-Path $destination 'amp-player.exe')) -eq $oldPlayer) 'Player rollback failed'
    Check ((File-Hash (Join-Path $destination 'addons\amp-engine.exe')) -eq $oldEngine) 'Engine rollback failed'
    Check ((File-Hash (Join-Path $destination ($moduleRelative+'\amp-stem-worker.exe'))) -eq $oldStemWorker) 'Previous stem worker was not restored'
    foreach ($name in $moduleFiles | Where-Object { $_ -ne 'amp-stem-worker.exe' }) { Check (!(Test-Path -LiteralPath (Join-Path $destination ($moduleRelative+'\'+$name)))) "Introduced module file was not removed: $name" }
    Check ((File-Hash $ownerAddon) -eq $ownerAddonHash) 'Rollback changed unrelated add-on'
    Check ((File-Hash $launcher) -eq $oldLauncher) 'Rollback changed launcher'
    Check ((File-Hash (Join-Path $settings 'saved.txt')) -eq $oldSettings) 'Rollback changed settings'
    Check (!(Test-Path -LiteralPath (Join-Path $destination 'tested-desktop-receipt.json'))) 'Introduced receipt was not removed'
    Check (!(Test-Path -LiteralPath (Join-Path $destination 'LICENSE'))) 'Introduced file was not removed'
    Write-Output 'Synthetic install/rollback checks passed. No real installation or binary execution occurred.'
} finally {
    $env:LOCALAPPDATA=$oldLocal; $env:APPDATA=$oldRoaming
    Write-Output "Retained isolated fixture files: $root"
}
