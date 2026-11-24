[CmdletBinding()]
param([ValidateSet('Window','Tray','Console','Background')][string]$Mode='Window')
$ErrorActionPreference='Stop'
$installRoot=$PSScriptRoot
$l=Get-Content -LiteralPath (Join-Path $installRoot 'installed-launch.json') -Raw -Encoding UTF8|ConvertFrom-Json
if($l.schema -ne  1 -or $l.role -notin  @('Desktop','Server')){throw 'Installed launch configuration is unavailable.'}
function Quote([string]$value){'"'+[regex]::Replace($value,'(\\+)$','$1$1')+'"'}
function Child([string]$exe,[string]$arguments,[bool]$hidden){
 $info=New-Object Diagnostics.ProcessStartInfo
 $info.FileName=$exe;$info.Arguments=$arguments;$info.WorkingDirectory=$installRoot
 $info.UseShellExecute=$false;$info.CreateNoWindow=$hidden
 $info.EnvironmentVariables['PATH']=$installRoot+';'+$env:PATH
 return $info
}
$stateRoot=[IO.Path]::GetFullPath([string]$l.profile)
if(!(Test-Path -LiteralPath $stateRoot -PathType Container)){throw 'Private installation profile is missing. Do not create a replacement identity.'}
if($l.role -eq 'Desktop'){
 $exe=Join-Path $installRoot 'amp-player.exe'
 $running=@(Get-CimInstance Win32_Process -Filter "Name='amp-player.exe'"|Where-Object{$_.ExecutablePath -eq $exe})
 if($running.Count){Write-Output 'AMP Verse Desktop is already running.';return}
 $info=Child $exe (Quote $l.music) $true
 $info.EnvironmentVariables['AMP_V1_MODE']='1'
 $info.EnvironmentVariables['AMP_VERSE_CONFIG_DIR']=Join-Path $stateRoot 'settings'
 $info.EnvironmentVariables['APPDATA']=Join-Path $stateRoot 'AppData'
 $info.EnvironmentVariables['AMP_DOCUMENTS_DIR']=Join-Path $stateRoot 'Documents'
 $info.EnvironmentVariables['AMP_DEVICE_CONFIG_DIR']=Join-Path $stateRoot 'device'
 $info.EnvironmentVariables['AMP_ENGINE_EXE']=Join-Path $installRoot 'addons\amp-engine.exe'
 $info.EnvironmentVariables.Remove('AMP_CAST_PASSWORD')
 [void][Diagnostics.Process]::Start($info)
 return
}
$exe=Join-Path $installRoot 'amp-cast.exe'
# The launcher owns this gate; it never takes ServerController's separate start mutex.
$gate=New-Object Threading.Mutex($false,('Local\AMPVerseServerLauncher-'+$(if($l.fixture){[IO.Path]::GetFileName((Split-Path $installRoot -Parent))}else{'Canonical'})))
$held=$false
try{
 try{$held=$gate.WaitOne(5000)}catch [Threading.AbandonedMutexException]{$held=$true}
 if(!$held){throw 'Another server start is in progress.'}
 $all=@(Get-CimInstance Win32_Process -Filter "Name='amp-cast.exe'")
 $same=@($all|Where-Object{$_.ExecutablePath -eq $exe})
 if(!$l.fixture -and $all.Count -and $same.Count -ne  1){throw 'Another server installation is running. No second server was started.'}
 if($same.Count -gt  1){throw 'Multiple instances of this server are running.'}
 if(!$same.Count){
  $info=Child $exe (Quote $l.music) ($Mode -ne 'Console')
  $info.EnvironmentVariables['AMP_CAST_CONFIG_DIR']=$stateRoot
  $info.EnvironmentVariables['AMP_DEVICE_CONFIG_DIR']=Join-Path $stateRoot 'device'
  $info.EnvironmentVariables['AMP_CAST_BIND']=$l.bind
  $info.EnvironmentVariables.Remove('AMP_CAST_PASSWORD')
  $auth=Get-Content -LiteralPath (Join-Path $stateRoot 'server-auth-mode.json') -Raw -Encoding UTF8|ConvertFrom-Json
  if($auth.protected){
   Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -ErrorAction Stop
   $secure=(Get-Content -LiteralPath (Join-Path $stateRoot 'lan-control.dpapi') -Raw).Trim()|ConvertTo-SecureString
   try{$credential=New-Object Net.NetworkCredential('',$secure);$info.EnvironmentVariables['AMP_CAST_PASSWORD']=$credential.Password}finally{$secure.Dispose();$credential=$null}
  }
  if($l.fixture){$info.EnvironmentVariables['AMP_CAST_DISCOVERY_PORT']='0'}
  $process=[Diagnostics.Process]::Start($info)
  if(!$process){throw 'Server could not start.'}
  Write-Output ("Started this installation's server PID "+$process.Id)
 }else{Write-Output 'This server installation is already running.'}
}finally{if($held){$gate.ReleaseMutex()};$gate.Dispose()}
if($Mode -eq 'Console'){
 if($process){$process.WaitForExit();exit $process.ExitCode}
 return
}
if($Mode -eq 'Background'){return}
$manager=Join-Path $installRoot 'manager\amp-server-manager.exe'
if(!(Test-Path -LiteralPath $manager -PathType Leaf)){throw 'The optional server manager is unavailable; the server continues headlessly.'}
$managerProfile=Join-Path $stateRoot 'manager.json'
if(!(Test-Path -LiteralPath $managerProfile)){
 @{Executable=$exe;MusicDirectory=$l.music;ConfigDirectory=$stateRoot;DeviceDirectory=(Join-Path $stateRoot 'device');Bind=$l.bind;WebUrl=$l.web_url;UseInstalledLauncher=$true}|ConvertTo-Json|Set-Content -LiteralPath $managerProfile -Encoding UTF8
}
$argument=if($Mode -eq 'Tray'){'--tray'}else{'--window'}
$info=Child $manager ($argument+' '+(Quote $managerProfile)) $true
[void][Diagnostics.Process]::Start($info)
