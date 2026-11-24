[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference='Stop'
$Destination=[IO.Path]::GetFullPath($Destination)
if(Test-Path -LiteralPath $Destination){throw 'CRT export destination must be new; retain earlier receipts.'}
$vswhere=Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
if(!(Test-Path -LiteralPath $vswhere -PathType Leaf)){throw 'Official Visual Studio redistributables are required on the packaging runner.'}
$installation=(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath|Select-Object -First 1)
if(!$installation){throw 'No Visual Studio C++ toolchain was found.'}
$version=(Get-Content -LiteralPath (Join-Path $installation 'VC\Auxiliary\Build\Microsoft.VCRedistVersion.default.txt') -Raw).Trim()
if($version -notmatch '^[0-9.]+$'){throw 'Invalid Visual Studio redist version.'}
$redist=Join-Path $installation ('VC\Redist\MSVC\'+$version)
$folders=@(Get-ChildItem -LiteralPath (Join-Path $redist 'x64') -Directory -Filter 'Microsoft.VC*.CRT')
if($folders.Count -ne 1){throw 'Expected one official x64 CRT redistributable directory.'}
$source=$folders[0].FullName
if($source -match 'debug_nonredist|System32'){throw 'Nonredistributable or ambient runtime source refused.'}
$allowed=@('concrt140.dll','msvcp140.dll','msvcp140_1.dll','msvcp140_2.dll','msvcp140_atomic_wait.dll','msvcp140_codecvt_ids.dll','vccorlib140.dll','vcruntime140.dll','vcruntime140_1.dll','vcruntime140_threads.dll')
$files=@(Get-ChildItem -LiteralPath $source -File)
foreach($required in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')){if(!($files|Where-Object Name -eq $required)){throw "Required CRT file missing: $required"}}
$plan=@()
foreach($file in $files){
 if($file.Name -notin $allowed -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Unexpected/linked CRT payload.'}
 $signature=Get-AuthenticodeSignature -LiteralPath $file.FullName
 if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation'){throw "Microsoft signature verification failed: $($file.Name)"}
 $plan+=@{name=$file.Name;source=$file.FullName;sha256=(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant();version=$file.VersionInfo.FileVersion;signer=$signature.SignerCertificate.Subject}
}
New-Item -ItemType Directory -Path $Destination|Out-Null
$notices=Join-Path $Destination 'notices'
New-Item -ItemType Directory -Path $notices|Out-Null
foreach($file in $plan){
 Copy-Item -LiteralPath $file.source -Destination (Join-Path $Destination $file.name)
 if((Get-FileHash -LiteralPath (Join-Path $Destination $file.name)).Hash.ToLowerInvariant() -cne $file.sha256){throw 'CRT copy hash mismatch.'}
}
$thirdParty=Join-Path $installation 'Licenses\BuildTools\1033\ThirdPartyNotices.txt'
if(!(Test-Path -LiteralPath $thirdParty)){throw 'Installed Microsoft notices were not found; do not publish this export.'}
Copy-Item -LiteralPath $thirdParty -Destination (Join-Path $notices 'Microsoft-ThirdPartyNotices.txt')
@'
Microsoft Visual C++ runtime files are copied unmodified from the installed
Visual Studio VC/Redist/MSVC/<version>/x64/Microsoft.VC*.CRT directory.
Redistribution is subject to the applicable Microsoft Software License Terms.
Official Distributable Code list:
https://learn.microsoft.com/visualstudio/releases/2022/redistribution
Official application-local deployment guidance:
https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
The original signed vc_redist.x64.exe is retained as the upstream distribution
and license container for inspection, never executed by the AMP installer.
No debug_nonredist, System32 or owner application DLL is included.
'@|Set-Content -LiteralPath (Join-Path $notices 'MICROSOFT-REDISTRIBUTION.txt') -Encoding ASCII
$original=Join-Path $redist 'vc_redist.x64.exe'
$sig=Get-AuthenticodeSignature -LiteralPath $original
if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation'){throw 'Original Microsoft redist signature was not valid.'}
Copy-Item -LiteralPath $original -Destination (Join-Path $notices 'vc_redist.x64.exe')
$receipt=@{schema=1;vs_installation=$installation;redist_version=$version;source_directory=$source;files=$plan;original_redist_sha256=(Get-FileHash -LiteralPath $original).Hash.ToLowerInvariant();source_policy='Official VS redistributable directory only; no global install or owner runtime replacement';license_reference='https://learn.microsoft.com/visualstudio/releases/2022/redistribution'}
[IO.File]::WriteAllText((Join-Path $Destination 'CRT-PROVENANCE.json'),($receipt|ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
Write-Output "Exported verified app-local Microsoft CRT to $Destination"
