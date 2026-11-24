[CmdletBinding()]
param([string]$PackageRoot=$PSScriptRoot,[string]$MusicDirectory,[string]$FixtureRoot,[switch]$NoStart,[switch]$CreateShortcuts,[switch]$ProtectedMode,[switch]$LoopbackOnly,[switch]$StartAfterInstall)
$ErrorActionPreference='Stop'
function Full([string]$p){$value=[IO.Path]::GetFullPath($p);if($value -eq [IO.Path]::GetPathRoot($value)){return $value};$value.TrimEnd('\')}
function Inside([string]$p,[string]$root){$p.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)}
function NoLinks([string]$p){
 $p=Full $p
 while($p){if((Test-Path -LiteralPath $p) -and ((Get-Item -Force -LiteralPath $p).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw "Linked path refused: $p"};$p=Split-Path $p -Parent}
}
function Hash([string]$p){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function MoveOwned([string]$source,[string]$target,[string]$parent){
 $source=Full $source;$target=Full $target;$parent=Full $parent
 if(!(Inside $source $parent) -or !(Inside $target $parent) -or (Test-Path -LiteralPath $target)){throw 'Owned staging move escaped its parent or would overwrite a path.'}
 NoLinks $source;NoLinks $target
 Move-Item -LiteralPath $source -Destination $target
}
function Json([string]$p,$value){[IO.File]::WriteAllText($p,($value|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))}
$PackageRoot=Full $PackageRoot
$m=Get-Content -LiteralPath (Join-Path $PackageRoot 'install-manifest.json') -Raw -Encoding UTF8|ConvertFrom-Json
if($m.schema -ne  1  -or  $m.role -notin  @('Desktop','Server')  -or  $m.source_commit -notmatch  '^[0-9a-f]{40}$'){throw 'Use a verified Windows installer package.'}
$fixture=![string]::IsNullOrWhiteSpace($FixtureRoot)
foreach($entry in $m.support_files){
 $relative=[string]$entry.path
 if(!$relative -or [IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.?([\\/]|$)|:'){throw 'Unsafe support path.'}
 $supportRoot=Full (Join-Path $PackageRoot 'support')
 $file=Full (Join-Path $supportRoot $relative)
 if(!(Inside $file $supportRoot)){throw 'Escaping support path.'}
 NoLinks $file
 if(!(Test-Path -LiteralPath $file -PathType Leaf) -or (Hash $file) -cne $entry.sha256){throw 'Support payload verification failed.'}
}
if($fixture){
 $FixtureRoot=Full $FixtureRoot
 if($FixtureRoot -notmatch  '^D:\\(?:CodexBuildCache|fixtures)\\[^\\]'){throw 'FixtureRoot must name an explicit disposable child of D:\CodexBuildCache or D:\fixtures.'}
 if($CreateShortcuts){throw 'Fixture installs do not create shortcuts.'}
 NoLinks $FixtureRoot
 $dest=Join-Path $FixtureRoot ('Programs\'+$m.role);$stateRoot=Join-Path $FixtureRoot ('State\'+$m.role)
}else{
 $dest=Join-Path $env:LOCALAPPDATA ('Programs\AMP Verse'+$(if($m.role -eq 'Server'){' Server'}else{''}))
 $stateRoot=Join-Path $env:LOCALAPPDATA $(if($m.role -eq 'Server'){'AMPVerse\StandaloneServer'}else{'AMPVerse\Desktop'})
}
$dest=Full $dest;$stateRoot=Full $stateRoot
NoLinks $dest;NoLinks $stateRoot
$payload=Full (Join-Path $PackageRoot 'payload')
$plan=@();$seen=@{}
foreach($f in $m.files){
 $r=[string]$f.path
 if(!$r -or [IO.Path]::IsPathRooted($r) -or $r-match '(^|[\\/])\.\.?([\\/]|$)|:'){throw 'Unsafe payload path.'}
 $src=Full (Join-Path $payload $r);$target=Full (Join-Path $dest $r)
 if(!(Inside $src $payload) -or !(Inside $target $dest) -or $seen.ContainsKey($target)){throw 'Duplicate or escaping payload path.'}
 NoLinks $src
 if(!(Test-Path -LiteralPath $src -PathType Leaf) -or [string]$f.sha256 -notmatch '^[0-9a-f]{64}$' -or (Hash $src) -cne $f.sha256){throw "Payload verification failed: $r"}
 $seen[$target]=$true;$plan+=@{Source=$src;Relative=$r;Hash=$f.sha256}
}
$exe=if($m.role -eq 'Server'){'amp-cast.exe'}else{'amp-player.exe'}
if(!$plan.Count -or !($plan|Where-Object Relative  -eq  $exe)){throw 'Required application payload missing.'}
foreach($r in @('ffmpeg.exe','ffprobe.exe','ffplay.exe','AUDIO_TOOL_NOTICES.md','launch-windows-installed.ps1')){
 if(!($plan|Where-Object Relative  -eq  $r)){throw "Required fresh runtime/launcher missing: $r"}
}
if(Test-Path -LiteralPath (Join-Path $dest $exe) -PathType Leaf){
 if($fixture){throw 'Retain this fixture and choose a new disposable root.'}
 if($m.role -eq 'Desktop'){
  $delivery=Join-Path $PackageRoot 'support\delivery-manifest.json'
  & (Join-Path $PackageRoot 'support\install-tested-desktop.ps1') -DesktopArtifact $payload -EngineArtifact (Join-Path $PackageRoot 'support\engine') -Manifest $delivery -ManifestSha256 (Hash $delivery) -SourceCommit $m.source_commit -RunId $m.run_id
 }else{& (Join-Path $payload 'install-windows-server.ps1') -InstallDirectory $dest}
 Write-Output 'Existing installation updated; its runtime, launcher and private identity were preserved.'
 return
}
if(!$fixture -and $m.role -eq 'Server'){
 $known=@(Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'Programs') -Directory -Filter 'AMP Verse Server*' -ErrorAction SilentlyContinue|Where-Object{Test-Path -LiteralPath (Join-Path $_.FullName 'amp-cast.exe') -PathType Leaf})
 if($known.Count){
  if($known.Count -ne  1){throw 'Multiple server installations exist; select the existing one explicitly.'}
  & (Join-Path $payload 'install-windows-server.ps1') -InstallDirectory $known[0].FullName
  return
 }
}
if((Test-Path -LiteralPath $dest) -or (Test-Path -LiteralPath $stateRoot)){throw 'Installation or private state already exists; use its update path instead of creating another identity.'}
if(!$fixture -and (Get-Process amp-player,amp-cast,amp-engine,amp-prepare,amp-update,amp-stem-worker,amp-server-manager -ErrorAction SilentlyContinue)){throw 'Close existing AMP applications first. Nothing was stopped.'}
if(!$MusicDirectory){
 if($NoStart){throw 'Supply -MusicDirectory for noninteractive installation.'}
 Add-Type -AssemblyName System.Windows.Forms
 $picker=New-Object Windows.Forms.FolderBrowserDialog;$picker.Description='Choose your existing music folder'
 if($picker.ShowDialog() -ne [Windows.Forms.DialogResult]::OK){throw 'Installation cancelled.'}
 $MusicDirectory=$picker.SelectedPath;$picker.Dispose()
}
$MusicDirectory=Full $MusicDirectory
if(!(Test-Path -LiteralPath $MusicDirectory -PathType Container)){throw 'An existing music folder is required.'}
$parent=Split-Path $dest -Parent
New-Item -ItemType Directory -Force -Path $parent|Out-Null
$stage=Join-Path $parent ('.amp-fresh-'+[Guid]::NewGuid().ToString('N'))
$stateStage=Join-Path (Split-Path $stateRoot -Parent) ('.amp-state-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $stage,$stateStage|Out-Null
try{
 foreach($f in $plan){$target=Join-Path $stage $f.Relative;New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent)|Out-Null;Copy-Item -LiteralPath $f.Source -Destination $target;if((Hash $target) -cne $f.Hash){throw 'Staged copy verification failed.'}}
 foreach($folder in @('settings','AppData','Documents','device')){New-Item -ItemType Directory -Path (Join-Path $stateStage $folder)|Out-Null}
 if($m.role -eq 'Server'){
  Json (Join-Path $stateStage 'server.json') @{version=1;server_id=[Guid]::NewGuid().ToString('D');station_id=[Guid]::NewGuid().ToString('D');station_name='AMP Verse Server';analysis_enabled=$false;sharing_enabled=$true;analysis_workers=1;scan_interval_seconds=60;pause_analysis_during_radio=$true}
  Json (Join-Path $stateStage 'device\device.json') @{version=1;id=[Guid]::NewGuid().ToString('D')}
 }
 if($m.role -eq 'Server'){
  Json (Join-Path $stateStage 'server-auth-mode.json') @{protected=[bool]$ProtectedMode}
 }
 if($m.role -eq 'Server' -and $ProtectedMode){
  Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -ErrorAction Stop
  $bytes=New-Object byte[] 24;$rng=[Security.Cryptography.RandomNumberGenerator]::Create()
  try{$rng.GetBytes($bytes)}finally{$rng.Dispose()}
  $secure=ConvertTo-SecureString ([Convert]::ToBase64String($bytes)) -AsPlainText -Force
  try{$secure|ConvertFrom-SecureString|Set-Content -LiteralPath (Join-Path $stateStage 'lan-control.dpapi') -Encoding ASCII}finally{$secure.Dispose();[Array]::Clear($bytes,0,$bytes.Length)}
 }
 $launch=@{schema=1;role=$m.role;music=$MusicDirectory;profile=$stateRoot;fixture=$fixture;bind=$(if($LoopbackOnly){'127.0.0.1:8787'}else{'0.0.0.0:8787'});web_url='http://127.0.0.1:8787/#cast'}
 if($fixture){$launch.bind='127.0.0.1:28991';$launch.web_url='http://127.0.0.1:28991/#cast'}
 Json (Join-Path $stage 'installed-launch.json') $launch
 Json (Join-Path $stage 'fresh-install-receipt.json') @{schema=1;source_commit=$m.source_commit;run_id=$m.run_id;role=$m.role;profile=$stateRoot;fixture=$fixture;started=$false;files=$m.files}
 NoLinks $dest;NoLinks $stateRoot
 if((Test-Path -LiteralPath $dest) -or (Test-Path -LiteralPath $stateRoot)){throw 'Destination appeared during staging.'}
 # Publish only newly owned paths. Failed staging is retained as recovery/audit data.
 MoveOwned $stateStage $stateRoot (Split-Path $stateRoot -Parent)
 try{MoveOwned $stage $dest (Split-Path $dest -Parent)}catch{
  if((Test-Path -LiteralPath $stateRoot) -and !(Test-Path -LiteralPath $stateStage)){MoveOwned $stateRoot $stateStage (Split-Path $stateRoot -Parent)}
  throw
 }
}catch{throw "Fresh installation failed; retained staging: $stage ; $stateStage . $($_.Exception.Message)"}
if($CreateShortcuts){
 $shell=New-Object -ComObject WScript.Shell
 $shortcut=$shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) ('AMP Verse'+$(if($m.role -eq 'Server'){' Server'}else{''})+'.lnk')))
 $shortcut.TargetPath=Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
 $shortcut.Arguments='-NoProfile -File "'+(Join-Path $dest 'launch-windows-installed.ps1')+'"'
 $shortcut.WorkingDirectory=$dest;$shortcut.Save()
}
Write-Output "Installed $($m.role) at $dest. Private state: $stateRoot. No startup task, service or firewall rule was created."
if($StartAfterInstall -and !$NoStart){& (Join-Path $dest 'launch-windows-installed.ps1')}
