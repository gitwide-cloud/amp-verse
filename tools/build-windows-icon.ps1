param([string]$ResourceCompiler = 'llvm-rc.exe')
$ErrorActionPreference='Stop'
Push-Location (Join-Path $PSScriptRoot '../assets')
try {
    & $ResourceCompiler /fo amp-verse.res amp-verse.rc
    if ($LASTEXITCODE -ne 0) {throw 'Windows resource compiler failed'}
} finally {Pop-Location}
