param([string]$Destination = "$env:LOCALAPPDATA/Programs/AMP Verse 1 Preview")
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$previous = Join-Path $env:LOCALAPPDATA 'Programs/AMP Verse'
if ([IO.Path]::GetFullPath($Destination).TrimEnd('\','/') -eq [IO.Path]::GetFullPath($previous).TrimEnd('\','/')) {
    throw 'The V1 preview must be installed separately from version 0.'
}
$binary = Join-Path $repo 'target/release/amp-player.exe'
if (!(Test-Path -LiteralPath $binary)) { throw 'Build the V1 player first.' }
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
# Copy runtime tools and their notices together; never alter the stable installation.
foreach ($name in @('ffmpeg.exe','ffprobe.exe','ffplay.exe','yt-dlp.exe','deno.exe','licenses','addons','tools','lab-tools','AUDIO_TOOL_NOTICES.md')) {
    if ($name -in @('addons','tools','lab-tools') -and (Test-Path -LiteralPath (Join-Path $Destination $name))) { continue }
    $source = Join-Path $previous $name
    if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination $Destination -Recurse -Force }
}
foreach ($name in @('amp-player.exe','amp-keylock.dll','amp-update.exe')) {
    Copy-Item -LiteralPath (Join-Path $repo "target/release/$name") -Destination $Destination -Force
}
foreach ($name in @('LICENSE','THIRD_PARTY_NOTICES.md','docs')) {
    Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $Destination -Recurse -Force
}
# An existing optional engine must move with the player to gain cooperative playback pauses.
$installedEngine=Join-Path $Destination 'addons/amp-engine.exe'
$builtEngine=Join-Path $repo 'target/release/amp-engine.exe'
if (Test-Path -LiteralPath $installedEngine) {
    if (!(Test-Path -LiteralPath $builtEngine)) { throw 'Build the matching amp-engine executable before updating this preview.' }
    Copy-Item -LiteralPath $builtEngine -Destination $installedEngine -Force
}
Copy-Item -LiteralPath (Join-Path $repo 'assets/amp-verse.ico') -Destination $Destination -Force
Copy-Item -LiteralPath (Join-Path $repo 'tools/launch-desktop.vbs') -Destination (Join-Path $Destination 'Launch AMP Verse.vbs') -Force
$shell = New-Object -ComObject WScript.Shell
foreach ($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))) {
    $shortcut = $shell.CreateShortcut((Join-Path $folder 'AMP Verse 1 Preview.lnk'))
    $shortcut.TargetPath = Join-Path $env:WINDIR 'System32/wscript.exe'
    $shortcut.Arguments = '"' + (Join-Path $Destination 'Launch AMP Verse.vbs') + '"'
    $shortcut.WorkingDirectory = $Destination
    $shortcut.IconLocation = (Join-Path $Destination 'amp-verse.ico') + ',0'
    $shortcut.Description = 'AMP Verse 1.0 preview — separate settings; version 0 remains installed'
    $shortcut.Save()
    if (Test-Path -LiteralPath (Join-Path $previous 'amp-player.exe')) {
        $back = $shell.CreateShortcut((Join-Path $folder 'AMP Verse v0.lnk'))
        $back.TargetPath = Join-Path $previous 'amp-player.exe'
        $back.WorkingDirectory = $previous
        $back.IconLocation = (Join-Path $previous 'amp-verse.ico') + ',0'
        $back.Description = 'Original AMP Verse — close the preview before returning here'
        $back.Save()
    }
}
Write-Output "Installed side-by-side V1 preview at $Destination. Version 0 was not modified."
