[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Artifact,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ManifestSha256,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'Programs/AMP Verse')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$files = @(
'dependency-inventory.json','NOTICES.md',
'LICENSES/NVIDIA-CUDA-12.4.0-EULA.html','LICENSES/NVIDIA-cuDNN-9.1.0-EULA.html',
'LICENSES/NVIDIA-license-provenance.json','LICENSES/ONNXRuntime-LICENSE.txt',
'LICENSES/ONNXRuntime-ThirdPartyNotices.txt','LICENSES/PyTorch-distribution-LICENSE.txt',
'LICENSES/PyTorch-distribution-NOTICE.txt','onnxruntime.dll','onnxruntime_providers_cuda.dll',
'onnxruntime_providers_shared.dll','vendor/cublas64_12.dll','vendor/cublasLt64_12.dll',
'vendor/cudart64_12.dll','vendor/cufft64_11.dll','vendor/cudnn64_9.dll','vendor/cudnn_adv64_9.dll',
'vendor/cudnn_cnn64_9.dll','vendor/cudnn_engines_precompiled64_9.dll',
'vendor/cudnn_engines_runtime_compiled64_9.dll','vendor/cudnn_graph64_9.dll',
'vendor/cudnn_heuristic64_9.dll','vendor/cudnn_ops64_9.dll')
$options = @{device_id='0';gpu_mem_limit='4294967296';arena_extend_strategy='kSameAsRequested';cudnn_conv_algo_search='HEURISTIC';cudnn_conv_use_max_workspace='1';use_tf32='0'}
function SafePath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if ($full -eq [IO.Path]::GetPathRoot($full)) { throw 'A filesystem root is not permitted' }
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse points are not permitted' }
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }; $cursor = $parent.FullName
    }
    return $full.TrimEnd([IO.Path]::DirectorySeparatorChar)
}
function Child([string]$Root,[string]$Path) {
    $full = SafePath $Path
    if (-not $full.StartsWith($Root + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Path leaves its verified parent' }
    return $full
}
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Verify([string]$Root,[string]$Pin) {
    $Root = SafePath $Root
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) { throw 'Package directory missing' }
    $mp = Child $Root (Join-Path $Root 'module.json')
    if (-not (Test-Path -LiteralPath $mp -PathType Leaf) -or (Get-Item -LiteralPath $mp).Length -gt 16384 -or (Hash $mp) -ne $Pin.ToLowerInvariant()) { throw 'Manifest checksum or size mismatch' }
    $m = Get-Content -LiteralPath $mp -Raw | ConvertFrom-Json
    if ($m.id -cne 'stems-cuda' -or $m.version -ne 1 -or $m.provider -cne 'cuda' -or $m.onnxruntime_version -cne '1.22.0' -or $m.model_sha256 -cne '98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845' -or $m.qualified -isnot [bool] -or -not $m.qualified) { throw 'Only pinned qualified CUDA packages can be installed' }
    if (@($m.provider_options.PSObject.Properties).Count -ne $options.Count) { throw 'Unexpected provider options' }
    foreach ($key in $options.Keys) { if ($m.provider_options.$key -cne $options[$key]) { throw 'Provider option mismatch' } }
    if (@($m.files).Count -ne $files.Count) { throw 'Incomplete payload' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    [long]$total = 0
    foreach ($entry in $m.files) {
        $name = [string]$entry.path
        if ($files -cnotcontains $name -or -not $seen.Add($name)) { throw 'Unknown or duplicate payload path' }
        $target = Child $Root (Join-Path $Root $name)
        $item = Get-Item -LiteralPath $target -Force
        if ($item.PSIsContainer -or $entry.bytes -isnot [long] -and $entry.bytes -isnot [int] -or $entry.bytes -le 0 -or $entry.bytes -gt 1GB -or $item.Length -ne $entry.bytes) { throw 'Payload size mismatch' }
        $total += $entry.bytes
        if ($total -gt 3GB -or $entry.sha256 -cnotmatch '^[a-fA-F0-9]{64}$' -or (Hash $target) -ne $entry.sha256.ToLowerInvariant()) { throw 'Payload checksum or budget mismatch' }
        if ($name.EndsWith('.dll',[StringComparison]::OrdinalIgnoreCase)) {
            $stream = [IO.File]::OpenRead($target)
            $reader = [IO.BinaryReader]::new($stream)
            try {
                if ($stream.Length -lt 90 -or $reader.ReadUInt16() -ne 0x5a4d) { throw 'Invalid MZ header' }
                $stream.Position = 60; $offset = $reader.ReadUInt32()
                if ($offset -lt 64 -or $offset -gt 1MB -or $offset -gt $stream.Length - 26) { throw 'Invalid PE offset' }
                $stream.Position = $offset
                if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'AMD64 PE required' }
                $stream.Position = $offset + 24
                if ($reader.ReadUInt16() -ne 0x20b) { throw 'PE32+ required' }
            } finally { $reader.Dispose() }
        }
    }
    $actual = 0
    # Check each directory before descending; never follow a linked member.
    $pending = [Collections.Generic.Queue[string]]::new(); $pending.Enqueue($Root)
    while ($pending.Count) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Dequeue() -Force) {
            $null = Child $Root $item.FullName
            if ($item.PSIsContainer) {
                $directory = $item.FullName.Substring($Root.Length + 1).Replace('\','/')
                if ($directory -cne 'LICENSES' -and $directory -cne 'vendor') { throw 'Unexpected package directory' }
                $pending.Enqueue($item.FullName); continue
            }
            $relative = $item.FullName.Substring($Root.Length + 1).Replace('\','/')
            if ($relative -cne 'module.json' -and -not $seen.Contains($relative)) { throw 'Unexpected package member' }
            $actual++
        }
    }
    if ($actual -ne 25 -or $m.payload_bytes -ne $total) { throw 'Payload accounting mismatch' }
    return $m
}
function AssertClosed {
    if (Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match '^(amp-player|amp-stem-worker|AMPVerse|AMP Verse)$' }) { throw 'Close AMP Verse and its stem worker before installing' }
}
$source = SafePath $Artifact
$app = SafePath $InstallRoot
if (-not (Test-Path -LiteralPath (Join-Path $app 'amp-player.exe') -PathType Leaf)) { throw 'An existing AMP Verse installation is required' }
$appStream = [IO.File]::OpenRead((Child $app (Join-Path $app 'amp-player.exe')))
$appReader = [IO.BinaryReader]::new($appStream)
try {
    if ($appStream.Length -lt 90 -or $appReader.ReadUInt16() -ne 0x5a4d) { throw 'Existing application is not a PE executable' }
    $appStream.Position = 60; $offset = $appReader.ReadUInt32()
    if ($offset -lt 64 -or $offset -gt 1MB -or $offset -gt $appStream.Length - 26) { throw 'Invalid application PE offset' }
    $appStream.Position = $offset
    if ($appReader.ReadUInt32() -ne 0x4550 -or $appReader.ReadUInt16() -ne 0x8664) { throw 'Existing AMD64 application required' }
    $appStream.Position = $offset + 24
    if ($appReader.ReadUInt16() -ne 0x20b) { throw 'Existing PE32+ application required' }
} finally { $appReader.Dispose() }
$manifest = Verify $source $ManifestSha256
AssertClosed
$addons = Child $app (Join-Path $app 'addons')
$target = Child $addons (Join-Path $addons 'stems-cuda')
if (Test-Path -LiteralPath $target) {
    $oldPin = Hash (Child $target (Join-Path $target 'module.json'))
    $null = Verify $target $oldPin
    if ($oldPin -eq $ManifestSha256.ToLowerInvariant()) {
        [pscustomobject]@{status='verified-skip';manifest_sha256=$oldPin;files=24;file_proof=$manifest.files;payload_bytes=$manifest.payload_bytes;path=$target} | ConvertTo-Json -Compress
        return
    }
}
New-Item -ItemType Directory -Path $addons -Force | Out-Null
$stage = Child $addons (Join-Path $addons ('.stems-cuda-stage-' + [guid]::NewGuid().ToString('N')))
$backup = $null; $published = $false
try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    foreach ($name in @('module.json') + $files) {
        $dest = Child $stage (Join-Path $stage $name)
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dest)) -Force | Out-Null
        Copy-Item -LiteralPath (Child $source (Join-Path $source $name)) -Destination $dest
    }
    $null = Verify $stage $ManifestSha256
    AssertClosed
    $null = Child $addons $target
    if (Test-Path -LiteralPath $target) {
        $null = Verify $target $oldPin
        $backupRoot = Child $app (Join-Path $app 'AMPVerseInstallBackupsGPU')
        New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null
        $backup = Child $backupRoot (Join-Path $backupRoot ('stems-cuda-' + [guid]::NewGuid().ToString('N')))
        Move-Item -LiteralPath (Child $addons $target) -Destination $backup
    }
    Move-Item -LiteralPath (Child $addons $stage) -Destination $target
    $published = $true
    $null = Verify $target $ManifestSha256
} catch {
    if ($published -and (Test-Path -LiteralPath $target)) { Remove-Item -LiteralPath (Child $addons $target) -Recurse -Force }
    if ($backup -and (Test-Path -LiteralPath $backup)) { Move-Item -LiteralPath (Child $app $backup) -Destination (Child $addons $target) }
    throw
} finally {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath (Child $addons $stage) -Recurse -Force }
}
[pscustomobject]@{status='installed';manifest_sha256=$ManifestSha256.ToLowerInvariant();files=24;file_proof=$manifest.files;payload_bytes=$manifest.payload_bytes;path=$target;backup=$backup} | ConvertTo-Json -Compress




