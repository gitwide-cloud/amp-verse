# Displays the existing owner's code locally; never changes credentials.
[CmdletBinding()]
param([string]$ConfigDirectory = (Join-Path $env:LOCALAPPDATA 'AMPVerse\StandaloneServer'))
$ErrorActionPreference = 'Stop'
function Read-AMPServerAccessCode {
    param([Parameter(Mandatory=$true)][string]$Directory)
    $file = Join-Path ([IO.Path]::GetFullPath($Directory)) 'lan-control.dpapi'
    if (!(Test-Path -LiteralPath $file -PathType Leaf)) {
        $modeFile = Join-Path $Directory 'server-auth-mode.json'
        if (Test-Path -LiteralPath $modeFile -PathType Leaf) {
            $mode = Get-Content -LiteralPath $modeFile -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($mode.protected -eq $false) { return $null }
        }
        throw 'Run this on the server PC as the Windows user who configured AMP Verse Server. No saved code was found.'
    }
    try {
        $protected = [IO.File]::ReadAllText($file).Trim()
        if (!$protected -or $protected.Length -gt 65536) { throw 'Invalid protected code' }
        return ConvertTo-SecureString -String $protected -ErrorAction Stop
    } catch {
        throw 'This Windows account cannot open the saved server access code. Use the Windows account that configured AMP Verse Server.'
    }
}
function Show-AMPServerAccessCode {
    param([string]$Directory)
    Add-Type -AssemblyName System.Windows.Forms
    $secure = $null
    $credential = $null
    $plain = $null
    try {
        $secure = Read-AMPServerAccessCode -Directory $Directory
        if ($null -eq $secure) {
            [void][Windows.Forms.MessageBox]::Show('This server uses code-free trusted-LAN access. No access code is required.', 'AMP Verse Server access', [Windows.Forms.MessageBoxButtons]::OK, [Windows.Forms.MessageBoxIcon]::Information)
            return
        }
        $credential = [Net.NetworkCredential]::new('', $secure)
        $plain = $credential.Password
        if (!$plain -or $plain.IndexOf([char]0) -ge 0) { throw 'Invalid saved code' }
        [void][Windows.Forms.MessageBox]::Show(
            "Enter this in AMP Verse Studio's Sign in screen on your own devices. This is the server access code, not your Windows password. Keep it private." + [Environment]::NewLine + [Environment]::NewLine + $plain,
            'AMP Verse Server - existing access code',
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Information)
    } catch {
        [void][Windows.Forms.MessageBox]::Show(
            'Could not read the existing code. Run this on the server PC as the Windows user who configured AMP Verse Server. No password was changed.',
            'AMP Verse Server access',
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Warning)
        throw 'Server access code unavailable for this Windows account.'
    } finally {
        if ($secure) { $secure.Dispose() }
        $plain = $null
        $credential = $null
    }
}
if ($MyInvocation.InvocationName -ne '.') { Show-AMPServerAccessCode -Directory $ConfigDirectory }
