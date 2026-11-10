param(
    [string]$Python = "python",
    [string]$Destination = "$env:LOCALAPPDATA/Programs/AMP Verse/addons/stems",
    [switch]$DownloadModel
)
$ErrorActionPreference = 'Stop'
# Isolated optional add-on. The player and ordinary analysis do not depend on it.
& $Python -c "import sys; assert sys.version_info[:2] == (3, 12), 'Use Python 3.12 for this pinned add-on'"
if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 is required.' }
& $Python -m venv $Destination
if ($LASTEXITCODE -ne 0) { throw 'Could not create the stem add-on environment.' }
$stemPython = Join-Path $Destination 'Scripts/python.exe'
& $stemPython -m pip install torch==2.6.0 torchaudio==2.6.0 --index-url https://download.pytorch.org/whl/cpu
if ($LASTEXITCODE -ne 0) { throw 'Could not install the CPU audio runtime.' }
& $stemPython -m pip install demucs==4.0.1 'numpy<2'
if ($LASTEXITCODE -ne 0) { throw 'Could not install Demucs.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../apps/amp-player/src/stem_worker.py') -Destination (Join-Path $Destination 'stem_worker.py') -Force
if ($DownloadModel) {
    $priorTorchHome = $env:TORCH_HOME
    try {
        $env:TORCH_HOME = Join-Path $Destination 'models'
        & $stemPython (Join-Path $Destination 'stem_worker.py') --preload
        if ($LASTEXITCODE -ne 0) { throw 'The stem model could not be prepared.' }
    } finally { $env:TORCH_HOME = $priorTorchHome }
}
Write-Output "Installed optional stem add-on at $Destination"
