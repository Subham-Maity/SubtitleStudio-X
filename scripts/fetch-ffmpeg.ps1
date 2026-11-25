param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$BinDir = Join-Path $RepoRoot "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

$FfmpegTarget = Join-Path $BinDir "ffmpeg-x86_64-pc-windows-msvc.exe"
$FfprobeTarget = Join-Path $BinDir "ffprobe-x86_64-pc-windows-msvc.exe"

if (-not $Force -and (Test-Path $FfmpegTarget) -and (Test-Path $FfprobeTarget)) {
    Write-Host "FFmpeg sidecars already present in $BinDir"
    exit 0
}

$Urls = @(
    "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-gpl-8.1.zip",
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
)
$TempZip = Join-Path $env:TEMP "ffmpeg-win64-gpl.zip"
$TempExtract = Join-Path $env:TEMP "ffmpeg-extract"

$Downloaded = $false
foreach ($Url in $Urls) {
    Write-Host "Downloading FFmpeg Windows static build from $Url..."
    & curl.exe -fL --connect-timeout 20 --max-time 240 -o $TempZip $Url
    if ($LASTEXITCODE -eq 0 -and (Test-Path $TempZip) -and ((Get-Item $TempZip).Length -gt 10MB)) {
        $Downloaded = $true
        break
    }
    Write-Warning "Download from $Url failed (exit $LASTEXITCODE); trying next mirror..."
}

if (-not $Downloaded) {
    throw "Failed to download FFmpeg static build from all mirrors."
}

if (Test-Path $TempExtract) { Remove-Item -Recurse -Force $TempExtract }
Expand-Archive -Path $TempZip -DestinationPath $TempExtract -Force

$FfmpegExe = Get-ChildItem -Path $TempExtract -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
$FfprobeExe = Get-ChildItem -Path $TempExtract -Recurse -Filter "ffprobe.exe" | Select-Object -First 1

if (-not $FfmpegExe -or -not $FfprobeExe) {
    throw "Could not find ffmpeg.exe or ffprobe.exe inside downloaded archive"
}

Copy-Item -Path $FfmpegExe.FullName -Destination $FfmpegTarget -Force
Copy-Item -Path $FfprobeExe.FullName -Destination $FfprobeTarget -Force

Remove-Item -Force $TempZip -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $TempExtract -ErrorAction SilentlyContinue

Write-Host "Installed sidecars:"
Write-Host "  $FfmpegTarget"
Write-Host "  $FfprobeTarget"
