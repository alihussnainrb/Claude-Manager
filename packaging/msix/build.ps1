<#
.SYNOPSIS
  Build Claude Manager's MSIX layout, then register it for local testing or pack it for the Store.

.EXAMPLE
  # Local test (needs Developer Mode): register the unsigned layout for this user.
  .\packaging\msix\build.ps1 -Exe target\release\claude-manager.exe -Register

.EXAMPLE
  # Store upload: identity values come from Partner Center (Product identity page).
  .\packaging\msix\build.ps1 -Exe target\release\claude-manager.exe -Version 0.2.0 -Pack `
    -IdentityName "12345Example.ClaudeManager" -Publisher "CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" `
    -PublisherDisplayName "Your Name"
#>
param(
  [Parameter(Mandatory)] [string]$Exe,
  [string]$Version = "0.0.0",
  [string]$IdentityName = "ClaudeManager.Dev",
  [string]$Publisher = "CN=ClaudeManagerDev",
  [string]$PublisherDisplayName = "Claude Manager contributors",
  [string]$OutDir = "dist",
  # Separate from the dev layout so packing for the Store doesn't disturb a registered dev build.
  [string]$LayoutDir = "target\msix-layout",
  [switch]$Register,
  [switch]$Pack
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$layout = Join-Path $root $LayoutDir

# MSIX versions are four numeric parts; the fourth must be 0 for the Store.
$v = ($Version.TrimStart('v').Split('-')[0].Split('.') + @('0', '0', '0'))[0..2] -join '.'
$msixVersion = "$v.0"

if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $layout "Assets") | Out-Null
Copy-Item (Resolve-Path $Exe) (Join-Path $layout "claude-manager.exe")

# ---- Tile and logo images: the app's two-squares mark in coral ------------------------------
Add-Type -AssemblyName System.Drawing
function New-Logo([int]$w, [int]$h, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'
  $g.Clear([System.Drawing.Color]::Transparent)
  $s = [Math]::Min($w, $h) / 24.0 * 0.8           # the mark fills 80% of the shorter side
  $ox = ($w - 24 * $s) / 2; $oy = ($h - 24 * $s) / 2
  $coral = [System.Drawing.Color]::FromArgb(255, 0xE2, 0x72, 0x5B)
  $pen = New-Object System.Drawing.Pen $coral, (2 * $s)
  function RRect([double]$x, [double]$y, [double]$size, [double]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $size - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $size - $d, $y + $size - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $size - $d, $d, $d, 90, 90)
    $p.CloseFigure(); $p
  }
  $back = RRect ($ox + 9 * $s) ($oy + 3 * $s) (12 * $s) (3.5 * $s)
  $front = RRect ($ox + 3 * $s) ($oy + 7 * $s) (12 * $s) (3.5 * $s)
  $fill = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(77, $coral))
  $g.FillPath($fill, $back); $g.DrawPath($pen, $back)
  # Clear the front square's inside so it reads as "in front".
  $g.CompositingMode = 'SourceCopy'
  $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::Transparent)), $front)
  $g.CompositingMode = 'SourceOver'
  $g.DrawPath($pen, $front)
  $g.Dispose(); $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
}
$assets = Join-Path $layout "Assets"
New-Logo 44 44 (Join-Path $assets "Square44x44Logo.png")
New-Logo 150 150 (Join-Path $assets "Square150x150Logo.png")
New-Logo 310 150 (Join-Path $assets "Wide310x150Logo.png")
New-Logo 50 50 (Join-Path $assets "StoreLogo.png")
# Unplated taskbar/Start icon so Windows doesn't draw it on an accent-colored plate.
New-Logo 44 44 (Join-Path $assets "Square44x44Logo.targetsize-44_altform-unplated.png")

# ---- Manifest -------------------------------------------------------------------------------
$manifest = Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml.in") -Raw
$manifest = $manifest.Replace("{{IdentityName}}", $IdentityName).Replace("{{Publisher}}", $Publisher).
  Replace("{{PublisherDisplayName}}", $PublisherDisplayName).Replace("{{Version}}", $msixVersion)
[IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest, (New-Object Text.UTF8Encoding $false))
"Layout ready: $layout (version $msixVersion)"

if ($Register) {
  # Unsigned loose-layout registration: Developer Mode only.
  Get-AppxPackage -Name $IdentityName | Remove-AppxPackage -ErrorAction SilentlyContinue
  Add-AppxPackage -Register (Join-Path $layout "AppxManifest.xml")
  $pkg = Get-AppxPackage -Name $IdentityName
  "Registered $($pkg.PackageFullName)"
}

if ($Pack) {
  $sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" | Sort-Object FullName | Select-Object -Last 1
  if (-not $sdk) { throw "makeappx.exe not found; install the Windows SDK." }
  New-Item -ItemType Directory -Force (Join-Path $root $OutDir) | Out-Null
  $out = Join-Path $root "$OutDir\ClaudeManager-$v.msix"
  & $sdk.FullName pack /d $layout /p $out /o | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
  "Packed $out (unsigned; the Store signs it on submission)"
}
