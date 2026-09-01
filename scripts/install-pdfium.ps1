# install-pdfium.ps1
# ============================================================
#  FreeDF — installs the PDFium DLL next to the FreeDF executables.
#
#  FreeDF needs `pdfium.dll` next to the executable to open PDF files.
#  This script downloads the latest prebuilt PDFium (bblanchon/pdfium-binaries)
#  and copies the DLL to:
#    - the project root
#    - target\release  (if it exists)
#    - target\debug    (if it exists)
#  or to a directory you pass with -TargetDir.
#
#  Usage:
#    .\scripts\install-pdfium.ps1
#    .\scripts\install-pdfium.ps1 -TargetDir C:\apps\FreeDF
#    .\scripts\install-pdfium.ps1 -Arch arm64
# ============================================================

[CmdletBinding()]
param(
    # Optional explicit install directory (e.g. -TargetDir C:\apps\FreeDF)
    [string]$TargetDir = "",

    # CPU architecture: x64 (default), x86, arm64, etc.
    [string]$Arch = "x64"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

# GitHub API requires TLS 1.2 (needed on older Windows PowerShell 5.1)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Write-Host "== FreeDF PDFium installer ==" -ForegroundColor Cyan

# 1) Resolve the latest release and pick the Windows asset -----------------
$apiUrl = "https://api.github.com/repos/bblanchon/pdfium-binaries/releases/latest"
Write-Host "Resolving latest PDFium release..."
$release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "FreeDF-installer" }

# Recent releases name the assets pdfium-win-<arch>.tgz; older ones used
# pdfium-windows-<arch>.tgz. Try the current name first, then the legacy one.
$assetName = "pdfium-win-$Arch.tgz"
$asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -eq "pdfium-windows-$Arch.tgz" } | Select-Object -First 1
}
if (-not $asset) {
    throw "Could not find asset '$assetName' in release $($release.tag_name)."
}
$url = $asset.browser_download_url
$tmp = Join-Path $env:TEMP "pdfium-$Arch.tgz"
$extract = Join-Path $env:TEMP "pdfium-extract"

Write-Host "Downloading: $url"
curl.exe -L --fail --output $tmp $url
if ($LASTEXITCODE -ne 0) {
    throw "Download failed (curl exit code: $LASTEXITCODE). Check your internet connection."
}

# 2) Extract --------------------------------------------------------------
if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
New-Item -ItemType Directory -Force -Path $extract | Out-Null
Write-Host "Extracting archive..."
tar -xzf $tmp -C $extract
if ($LASTEXITCODE -ne 0) {
    throw "Extract failed (tar exit code: $LASTEXITCODE). On older Windows run in PowerShell 5.1+ or update tar."
}

$dll = Get-ChildItem -Recurse -Path $extract -Filter "pdfium.dll" | Select-Object -First 1
if (-not $dll) {
    throw "pdfium.dll was not found inside the archive."
}

# 3) Copy to destinations -------------------------------------------------
$destinations = @()
if ($TargetDir) {
    $destinations += $TargetDir
} else {
    $destinations += $root
    foreach ($profile in @("release", "debug")) {
        $d = Join-Path $root "target\$profile"
        if (Test-Path $d) { $destinations += $d }
    }
}
# The app also looks in its data folder, so install there unconditionally.
if ($env:LOCALAPPDATA) {
    $destinations += Join-Path $env:LOCALAPPDATA "FreeDF"
}

foreach ($dest in ($destinations | Select-Object -Unique)) {
    if (-not (Test-Path $dest)) { New-Item -ItemType Directory -Force -Path $dest | Out-Null }
    Copy-Item $dll.FullName -Destination (Join-Path $dest "pdfium.dll") -Force
    Write-Host ("Installed pdfium.dll -> {0}" -f (Join-Path $dest "pdfium.dll")) -ForegroundColor Green
}

# 4) Cleanup ---------------------------------------------------------------
Remove-Item -Recurse -Force $extract
Remove-Item -Force $tmp

Write-Host ""
Write-Host "Done. Rebuild/restart FreeDF and press Ctrl+O to open a PDF." -ForegroundColor Cyan
Write-Host "If you installed to a custom folder, keep pdfium.dll next to freedf.exe."
