param(
    [string]$Destination = "$env:LOCALAPPDATA/Programs/AMP Verse",
    [string]$AudioTools = "",
    [switch]$PlayerOnly
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$binary = Join-Path $repo 'target/release/amp-player.exe'
if (!(Test-Path -LiteralPath $binary)) { throw 'Build the release desktop binaries first.' }
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$binaries=if($PlayerOnly){@('amp-player.exe')}else{@('amp-player.exe','amp-cast.exe','amp-prepare.exe')}
foreach ($name in $binaries) {
    Copy-Item -LiteralPath (Join-Path $repo "target/release/$name") -Destination $Destination -Force
}
foreach ($name in @('README.md','LICENSE','THIRD_PARTY_NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $Destination -Force
}
Copy-Item -LiteralPath (Join-Path $repo 'docs') -Destination $Destination -Recurse -Force
if ($AudioTools) {
    foreach ($name in @('ffmpeg.exe','ffplay.exe','ffprobe.exe')) {
        Copy-Item -LiteralPath (Join-Path $AudioTools $name) -Destination $Destination -Force
    }
    if (Test-Path -LiteralPath (Join-Path $AudioTools 'licenses')) {
        Copy-Item -LiteralPath (Join-Path $AudioTools 'licenses') -Destination $Destination -Recurse -Force
    }
    Copy-Item -LiteralPath (Join-Path $AudioTools 'THIRD_PARTY_NOTICES.md') -Destination (Join-Path $Destination 'AUDIO_TOOL_NOTICES.md') -Force
}
$shell = New-Object -ComObject WScript.Shell
foreach ($folder in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
    $shortcut = $shell.CreateShortcut((Join-Path $folder 'AMP Verse.lnk'))
    $shortcut.TargetPath = Join-Path $Destination 'amp-player.exe'
    $shortcut.WorkingDirectory = $Destination
    $shortcut.Description = 'AMP Verse network audio player and DJ'
    $shortcut.Save()
}
Write-Output "Installed AMP Verse at $Destination"
