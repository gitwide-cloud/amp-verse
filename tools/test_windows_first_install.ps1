[CmdletBinding()]
param([string]$FixtureParent='D:\CodexBuildCache\amp-installer-tests')
$ErrorActionPreference='Stop'
$root=Join-Path $FixtureParent ([Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root -Force|Out-Null
function Json($path,$value){[IO.File]::WriteAllText($path,($value|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))}
function Assert($condition,$message){if(!$condition){throw $message}}
function Rejected([scriptblock]$action,$message){$rejected=$false;try{& $action|Out-Null}catch{$rejected=$true};Assert $rejected $message}
$package=Join-Path $root 'package';$payload=Join-Path $package 'payload'
New-Item -ItemType Directory -Path $payload -Force|Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'install-windows-package.ps1') -Destination $package
foreach($name in @('amp-player.exe','ffmpeg.exe','ffprobe.exe','ffplay.exe','AUDIO_TOOL_NOTICES.md','launch-windows-installed.ps1')){[IO.File]::WriteAllText((Join-Path $payload $name),'synthetic fixture only')}
$files=@(Get-ChildItem -LiteralPath $payload -File|ForEach-Object{@{path=$_.Name;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
$manifest=@{schema=1;role='Desktop';source_commit=('a'*40);run_id='1';files=$files;support_files=@()}
Json (Join-Path $package 'install-manifest.json') $manifest
$installer=Join-Path $package 'install-windows-package.ps1'
$music=Join-Path $root 'Música & mixes';New-Item -ItemType Directory -Path $music|Out-Null
$destination=Join-Path $root 'fresh'
& $installer -FixtureRoot $destination -MusicDirectory $music -NoStart|Out-Null
Assert (Test-Path -LiteralPath (Join-Path $destination 'Programs\Desktop\fresh-install-receipt.json')) 'Fresh receipt missing'
$launch=Get-Content -LiteralPath (Join-Path $destination 'Programs\Desktop\installed-launch.json') -Raw -Encoding UTF8|ConvertFrom-Json
Assert ($launch.music -eq $music) 'Unicode/space/ampersand music path changed'
Rejected {& $installer -FixtureRoot $destination -MusicDirectory $music -NoStart} 'Existing fixture was overwritten'
Rejected {& $installer -FixtureRoot '\\server\share\fixture' -MusicDirectory $music -NoStart} 'UNC fixture destination accepted'
Rejected {& $installer -FixtureRoot 'C:\Windows\fixture' -MusicDirectory $music -NoStart} 'Unsafe fixture destination accepted'
Rejected {& $installer -FixtureRoot (Join-Path $root 'shortcut') -MusicDirectory $music -NoStart -CreateShortcuts} 'Fixture shortcuts accepted'
$manifest.files[0].path='../escaped.exe';Json (Join-Path $package 'install-manifest.json') $manifest
Rejected {& $installer -FixtureRoot (Join-Path $root 'traversal') -MusicDirectory $music -NoStart} 'Traversal accepted'
$manifest.files=$files;$manifest.files[0].path='amp-player.exe';$manifest.files[0].sha256=('0'*64);Json (Join-Path $package 'install-manifest.json') $manifest
Rejected {& $installer -FixtureRoot (Join-Path $root 'hash') -MusicDirectory $music -NoStart} 'Changed payload accepted'
# Cross-edition regression: PS7's module path must not break the PS5 launcher.
$script=Join-Path $PSScriptRoot 'launch-windows-installed.ps1'
$text=[IO.File]::ReadAllText($script)
Assert (!$text.Contains("EnvironmentVariables['AMP_ANALYSIS_ENABLED']")) 'Launcher overrides persisted analysis setting'
Assert ($text.Contains("Join-Path `$PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1'")) 'Missing edition-owned Security module import'
$probe=Join-Path $root 'dpapi-probe.ps1'
@'
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1')
$secure=ConvertTo-SecureString 'synthetic test value' -AsPlainText -Force
$encoded=($secure|ConvertFrom-SecureString)+"`r`n"
$decoded=$encoded.Trim()|ConvertTo-SecureString
$credential=New-Object Net.NetworkCredential('',$decoded)
if($credential.Password -ne 'synthetic test value'){throw 'DPAPI round trip failed'}
$secure.Dispose();$decoded.Dispose()
'@|Set-Content -LiteralPath $probe -Encoding UTF8
& (Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe') -NoProfile -NonInteractive -File $probe
Assert ($LASTEXITCODE -eq 0) 'PS5 DPAPI child failed'
Write-Output "PASS 10 installer guards/launch regressions; retained synthetic evidence: $root"
