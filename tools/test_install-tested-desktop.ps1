# Bounded installer boundary checks. Extract functions only; never execute installation.
param([string]$Installer = (Join-Path $PSScriptRoot 'install-tested-desktop.ps1'))
$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($Installer,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] },$false)) {
    Invoke-Expression $definition.Extent.Text
}
function Must-Reject([scriptblock]$Action) {
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'Invalid input was accepted' }
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('amp-installer-check-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $pe = New-Object byte[] 128
    $pe[0]=77; $pe[1]=90; $pe[60]=64; $pe[64]=80; $pe[65]=69; $pe[68]=100; $pe[69]=134
    $valid = Join-Path $root 'valid.exe'
    [IO.File]::WriteAllBytes($valid,$pe)
    Assert-PE $valid
    $hash = (Get-FileHash -LiteralPath $valid -Algorithm SHA256).Hash
    $entries = @([pscustomobject]@{ path='valid.exe'; sha256=$hash })
    $verified = @(Verified-Files $root $entries)
    if ($verified.Count -ne 1 -or $verified[0].Hash -ne $hash) { throw 'Valid artifact did not verify' }
    Must-Reject { Verified-Files $root @([pscustomobject]@{ path='..\valid.exe'; sha256=$hash }) }
    Must-Reject { Verified-Files $root @([pscustomobject]@{ path='valid.exe'; sha256=('0'*64) }) }
    Must-Reject { Verified-Files $root @($entries[0],$entries[0]) }
    Must-Reject { Verified-Files $root @([pscustomobject]@{ path='valid.exe:alternate'; sha256=$hash }) }
    $copy = Join-Path $root 'copy.exe'
    Copy-Verified $valid $copy $hash
    $pe[69]=0
    $bad = Join-Path $root 'invalid.exe'
    [IO.File]::WriteAllBytes($bad,$pe)
    Must-Reject { Assert-PE $bad }
    if (!(Is-Within (Join-Path $root 'child') $root) -or (Is-Within ($root+'-neighbor') $root)) { throw 'Containment guard accepted a path prefix collision' }
    $stemNames = @('amp-stem-worker.exe','onnxruntime.dll','scnet.int8.onnx','NOTICES.md','module.json','LICENSES\SCNet-MIT.txt','LICENSES\ONNXRuntime-LICENSE.txt','LICENSES\ONNXRuntime-ThirdPartyNotices.txt','LICENSES\AMP-Verse-LICENSE.txt')
    $module = @($stemNames | ForEach-Object { [pscustomobject]@{ Source=$valid; Relative=('addons\stems-live\'+$_); Hash=$hash } })
    if (@(Get-StemLiveFiles $module).Count -ne 9) { throw 'Complete verified native module was not selected' }
    if (@(Get-StemLiveFiles @([pscustomobject]@{ Relative='addons\owner-addon\keep.dll' })).Count) { throw 'Unrelated add-on was selected' }
    Must-Reject { Get-StemLiveFiles @($module | Select-Object -Skip 1) }
    Must-Reject { Get-StemLiveFiles @($module + $module[0]) }
    Must-Reject { Get-StemLiveFiles @($module + [pscustomobject]@{ Relative='addons\stems-live\unreviewed.dll' }) }
    Must-Reject { Get-StemLiveFiles @($module + [pscustomobject]@{ Relative='addons\stems-live\Scripts\python.exe' }) }
    Must-Reject { Get-StemLiveFiles @($module + [pscustomobject]@{ Relative='addons\stems-live\LICENSES\other.txt' }) }
    Write-Output 'Passed: x64 PE, exact checksum/copy, traversal, duplicate, alternate stream, wrong machine and path containment checks. No installation ran.'
} finally {
    foreach ($name in @('valid.exe','copy.exe','invalid.exe')) {
        $file = Join-Path $root $name
        if (Test-Path -LiteralPath $file -PathType Leaf) { Remove-Item -LiteralPath $file }
    }
    Remove-Item -LiteralPath $root
}
