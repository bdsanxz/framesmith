# Framesmith one-command installer (Windows x64).
# Installs the packaged binary into PREFIX\bin after verifying the package
# checksum against release metadata. pwsh / powershell; no admin step.
#
# Usage: .\install.ps1 [-Prefix DIR] [-Force]
#
# The installer:
#   1. Refuses with the supported set on an unsupported OS/arch.
#   2. Verifies the binary's SHA-256 against the adjacent checksums.txt
#      (release metadata) before copying; a mismatch refuses and leaves
#      any previously installed binary untouched.
#   3. Copies the binary (plus notices) into PREFIX\bin.
#   4. Does NOT fetch FFmpeg: the installed binary bootstraps FFmpeg on
#      first render through the existing pinned-registry flow.
#
# Exit 0 = installed and verified. Non-zero = refused (named cause).

param(
  [string]$Prefix = (Join-Path $env:LOCALAPPDATA "framesmith"),
  [switch]$Force
)

$ErrorActionPreference = "Stop"

function Fail([string]$msg) {
  Write-Error "install.ps1: $msg"
  exit 1
}

# --- platform gate: refuse naming the supported set -------------------
# Windows x64 is the only Windows target. PowerShell on non-Windows
# (macOS/Linux) should use install.sh instead.
if ($PSVersionTable.PSVersion.Major -ge 7) {
  # pwsh can run on any OS; only allow it on Windows.
  if ($IsWindows -eq $false) {
    Fail "unsupported platform $($PSVersionTable.OS) (pwsh on non-Windows). Use install.sh for macOS/Linux."
  }
} else {
  if ($env:OS -ne "Windows_NT") {
    Fail "unsupported platform (not Windows). Use install.sh for macOS/Linux."
  }
}

# Architecture gate: only AMD64/x64 ships on Windows.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64" -and $arch -ne "x64") {
  Fail "unsupported platform windows/$arch. Supported platforms: macos-arm64; macos-x64; windows-x64; linux-x64"
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinName = "framesmith.exe"
$BinSrc = Join-Path $ScriptDir $BinName
$Checksums = Join-Path $ScriptDir "checksums.txt"

if (-not (Test-Path -LiteralPath $BinSrc)) {
  Fail "missing binary $BinSrc (corrupt package?)"
}
if (-not (Test-Path -LiteralPath $Checksums)) {
  Fail "missing checksums.txt release metadata; refusing to install unverified."
}

# Extract expected hex for the binary.
$Expected = $null
foreach ($line in (Get-Content -LiteralPath $Checksums)) {
  $t = $line.Trim()
  if ([string]::IsNullOrWhiteSpace($t) -or $t.StartsWith("#")) { continue }
  $parts = $t -split '\s+', 2
  if ($parts.Count -lt 2) { continue }
  if ($parts[1].Trim() -eq $BinName) { $Expected = $parts[0].Trim().ToLowerInvariant(); break }
}
if (-not $Expected) {
  Fail "no checksum entry for $BinName in checksums.txt; refusing."
}

$Actual = (Get-FileHash -LiteralPath $BinSrc -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Actual -ne $Expected) {
  Fail "checksum mismatch for ${BinName}: expected $Expected but found $Actual. Refusing; no files were copied and any existing install is untouched."
}

Write-Host "install.ps1: checksum verified ($BinName`: $Actual)"

# --- install into PREFIX ---------------------------------------------
$BinDir = Join-Path $Prefix "bin"
$ShareDir = Join-Path $Prefix "share\framesmith"

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $ShareDir | Out-Null

# Use Join-Path everywhere: PATH joins must use directory separators,
# not manual string concat, so Windows paths resolve correctly.
$Dest = Join-Path $BinDir $BinName
$Tmp = Join-Path $BinDir ".framesmith.tmp.$PID"
Copy-Item -LiteralPath $BinSrc -Destination $Tmp -Force
Move-Item -LiteralPath $Tmp -Destination $Dest -Force

Write-Host "install.ps1: installed $Dest"

# Notices travel with the binary (third-party / GPL ffmpeg-registry note).
foreach ($f in @("THIRD-PARTY-NOTICES.md", "LICENSE", "README.md")) {
  $src = Join-Path $ScriptDir $f
  if (Test-Path -LiteralPath $src) {
    Copy-Item -LiteralPath $src -Destination (Join-Path $ShareDir $f) -Force
  }
}

# Verify the installed binary runs.
try {
  $ver = (& $Dest --version 2>$null | Select-Object -First 1)
  if ($ver) { Write-Host "install.ps1: $ver" }
} catch {
  Write-Host "install.ps1: installed but version check failed; rendering may still work via bootstrap."
}

Write-Host "install.ps1: render a project with: $Dest render <folder>"
Write-Host "install.ps1: first render will download FFmpeg via the pinned registry if the host has none (network required once), then render offline."
Write-Host "install.ps1: add $BinDir to PATH if needed."
