# Offline transaction tests; fixture roots are always confined beneath workspace/test-output.
$ErrorActionPreference='Stop'
$installer=Join-Path $PSScriptRoot 'install-stem-cuda.ps1'
$workspace= [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
# In repository tools/, this resolves to the repository test-output directory.
$base=Join-Path $workspace 'test-output'
New-Item -ItemType Directory -Force -Path $base | Out-Null
$root=Join-Path $base ('stem-cuda-test-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$passed=0
# Isolate app-process checks from the user's running installation; production has no bypass.
function Get-Process { @() }
function Check($value,$message) { if (-not $value) {throw $message}; $script:passed++ }
function Reject($block,$message) { $rejected=$false; try { & $block | Out-Null } catch {$rejected=$true}; Check $rejected $message }
try {
 $source=Join-Path $root 'package'; $app=Join-Path $root 'AMP Verse'
 New-Item -ItemType Directory -Path $source,$app | Out-Null
 $pe=New-Object byte[] 128; $pe[0]=0x4d;$pe[1]=0x5a;$pe[60]=64;$pe[64]=0x50;$pe[65]=0x45;$pe[68]=0x64;$pe[69]=0x86;$pe[88]=0x0b;$pe[89]=2
 [IO.File]::WriteAllBytes((Join-Path $app 'amp-player.exe'),$pe)
 $text=Get-Content -LiteralPath $installer -Raw
 $list=[regex]::Match($text,'(?s)\$files = @\((.*?)\)\r?\n\$options').Groups[1].Value
 $names=@([regex]::Matches($list,"'([^']+)'") | ForEach-Object {$_.Groups[1].Value})
 $entries=@();$total=0
 foreach($name in $names) {
  $path=Join-Path $source $name; New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($path)) | Out-Null
  if($name.EndsWith('.dll')) {[IO.File]::WriteAllBytes($path,$pe)} else {[IO.File]::WriteAllText($path,'fixture')}
  $len=(Get-Item -LiteralPath $path).Length;$total+=$len
  $entries+=@{path=$name;bytes=$len;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
 }
 $m=@{id='stems-cuda';version=1;provider='cuda';onnxruntime_version='1.22.0';model_sha256='98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845';qualified=$true;provider_options=@{device_id='0';gpu_mem_limit='4294967296';arena_extend_strategy='kSameAsRequested';cudnn_conv_algo_search='HEURISTIC';cudnn_conv_use_max_workspace='1';use_tf32='0'};files=$entries;payload_bytes=$total}
 function Pin { $m | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $source 'module.json'); (Get-FileHash -LiteralPath (Join-Path $source 'module.json')).Hash }
 $pin=Pin
 function Get-Process { [pscustomobject]@{ProcessName='amp-player'} }
 Reject {& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app} 'Running app accepted'
 function Get-Process { @() }
 $result=& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app | ConvertFrom-Json
 Check ($result.status -eq 'installed') 'Initial installation failed'
 $installed=Join-Path $app 'addons/stems-cuda/module.json';$stamp=(Get-Item -LiteralPath $installed).LastWriteTimeUtc
 $result=& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app | ConvertFrom-Json
 Check ($result.status -eq 'verified-skip' -and (Get-Item -LiteralPath $installed).LastWriteTimeUtc -eq $stamp) 'Identical package was copied again'
 Reject {& $installer -Artifact $source -ManifestSha256 ('0'*64) -InstallRoot $app} 'Bad manifest pin accepted'
 $m.qualified=$false;$bad=Pin
 Reject {& $installer -Artifact $source -ManifestSha256 $bad -InstallRoot $app} 'Unqualified package accepted'
 $m.qualified=$true
 $dll=Join-Path $source 'onnxruntime.dll';$badPe=$pe.Clone();$badPe[68]=0
 [IO.File]::WriteAllBytes($dll,$badPe)
 $dllEntry=$m.files | Where-Object {$_.path -eq 'onnxruntime.dll'}
 $oldHash=$dllEntry.sha256;$dllEntry.sha256=(Get-FileHash -LiteralPath $dll).Hash.ToLowerInvariant();$bad=Pin
 Reject {& $installer -Artifact $source -ManifestSha256 $bad -InstallRoot $app} 'Non-AMD64 payload accepted despite matching hash'
 [IO.File]::WriteAllBytes($dll,$pe);$dllEntry.sha256=$oldHash
 $m.files[0].path='../escape';$bad=Pin
 Reject {& $installer -Artifact $source -ManifestSha256 $bad -InstallRoot $app} 'Traversal accepted'
 $m.files[0].path=$names[0];$m.files[1].path=$names[0];$bad=Pin
 Reject {& $installer -Artifact $source -ManifestSha256 $bad -InstallRoot $app} 'Duplicate accepted'
 $m.files[1].path=$names[1];$pin=Pin
 Set-Content -LiteralPath (Join-Path $source 'extra') -Value 'unexpected'
 Reject {& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app} 'Extra file accepted'
 Remove-Item -LiteralPath (Join-Path $source 'extra')
 [IO.File]::WriteAllText((Join-Path $source $names[0]),'tamper')
 Reject {& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app} 'Changed payload accepted'
 [IO.File]::WriteAllText((Join-Path $source $names[0]),'fixture')
 # A different valid package must preserve the prior verified installation as backup.
 $m.files[0].sha256=(Get-FileHash -LiteralPath (Join-Path $source $names[0])).Hash.ToLowerInvariant()
 $m['qualification_note']='offline fixture version two';$pin=Pin
 $result=& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app | ConvertFrom-Json
 Check ($result.status -eq 'installed' -and (Test-Path -LiteralPath (Join-Path $result.backup 'module.json'))) 'Replacement backup missing'
 Check ((Get-FileHash -LiteralPath $installed).Hash -eq $pin) 'Published package mismatch'
 $before=(Get-FileHash -LiteralPath $installed).Hash
 $m['qualification_note']='rollback fixture';$pin=Pin
 function Move-Item {
  param([string]$LiteralPath,[string]$Destination)
  if([IO.Path]::GetFileName($LiteralPath).StartsWith('.stems-cuda-stage-')) {throw 'Injected publication failure'}
  Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
 }
 Reject {& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $app} 'Injected transaction failure ignored'
 Remove-Item Function:Move-Item
 Check ((Get-FileHash -LiteralPath $installed).Hash -eq $before) 'Rollback did not restore prior package'
 Check (@(Get-ChildItem -LiteralPath (Join-Path $app 'addons') -Filter '.stems-cuda-stage-*').Count -eq 0) 'Staging left after failed transaction'
 $link=Join-Path $root 'linked-package'
 New-Item -ItemType Junction -Path $link -Target $source | Out-Null
 Reject {& $installer -Artifact $link -ManifestSha256 $pin -InstallRoot $app} 'Reparse root accepted'
 [IO.Directory]::Delete($link) # Delete only the junction; Windows PowerShell5.1 Remove-Item has a junction bug.
 Reject {& $installer -Artifact $source -ManifestSha256 $pin -InstallRoot $root} 'Missing app accepted'
 Write-Output "$passed offline installer checks passed"
} finally {
 $resolved=[IO.Path]::GetFullPath($root)
 if(-not $resolved.StartsWith([IO.Path]::GetFullPath($base)+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {throw 'Unsafe test cleanup path'}
 Remove-Item -LiteralPath $resolved -Recurse -Force
}




