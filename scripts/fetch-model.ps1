param(
    [ValidateSet("large-v3-turbo", "large-v3-turbo-q5_0", "small", "base")]
    [string]$Model = "large-v3-turbo",
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ModelsDir = Join-Path $env:APPDATA "SubtitleStudio\models"
New-Item -ItemType Directory -Force -Path $ModelsDir | Out-Null

$FileMap = @{
    "large-v3-turbo"      = "ggml-large-v3-turbo.bin"
    "large-v3-turbo-q5_0" = "ggml-large-v3-turbo-q5_0.bin"
    "small"               = "ggml-small.bin"
    "base"                = "ggml-base.bin"
}

$FileName = $FileMap[$Model]
$Target = Join-Path $ModelsDir $FileName
$Part = "$Target.part"

if (-not $Force -and (Test-Path $Target)) {
    Write-Host "Model already downloaded: $Target"
    exit 0
}

$Url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$FileName"
Write-Host "Downloading $Model ($FileName) to $Target..."
& curl.exe -fL -C - --connect-timeout 20 -o $Part $Url
if ($LASTEXITCODE -ne 0) {
    throw "Failed to download model from $Url (exit $LASTEXITCODE)"
}
Move-Item -Path $Part -Destination $Target -Force
Write-Host "Downloaded $Target"
