$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $RepoRoot

$VsDevCmd = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
if (Test-Path $VsDevCmd) {
    $envLines = cmd /c "`"$VsDevCmd`" -arch=amd64 -host_arch=amd64 -no_logo && set"
    foreach ($line in $envLines) {
        if ($line -match "^([^=]+)=(.*)$") {
            [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
        }
    }
}

if (-not $env:VULKAN_SDK) {
    $machineVulkan = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
    if ($machineVulkan -and (Test-Path $machineVulkan)) {
        $env:VULKAN_SDK = $machineVulkan
    } elseif (Test-Path "C:\VulkanSDK") {
        $latestVulkan = Get-ChildItem "C:\VulkanSDK" -Directory | Sort-Object Name -Descending | Select-Object -First 1
        if ($latestVulkan) { $env:VULKAN_SDK = $latestVulkan.FullName }
    }
}

$ExtraPaths = @(
    "C:\Program Files\LLVM\bin",
    "C:\Program Files\CMake\bin",
    "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja",
    "C:\Program Files (x86)\NSIS"
)
if ($env:VULKAN_SDK) {
    $ExtraPaths += (Join-Path $env:VULKAN_SDK "Bin")
}
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CMAKE_GENERATOR = "Ninja"
$env:PATH = (($ExtraPaths | Where-Object { Test-Path $_ }) -join ";") + ";" + $env:PATH

try {
    & (Join-Path $PSScriptRoot "fetch-ffmpeg.ps1")
    $debugDir = Join-Path $RepoRoot "target\debug"
    if (Test-Path $debugDir) {
        Copy-Item -Force (Join-Path $RepoRoot "src-tauri\binaries\ffmpeg-x86_64-pc-windows-msvc.exe") (Join-Path $debugDir "ffmpeg.exe")
        Copy-Item -Force (Join-Path $RepoRoot "src-tauri\binaries\ffprobe-x86_64-pc-windows-msvc.exe") (Join-Path $debugDir "ffprobe.exe")
    }
    npx --yes @tauri-apps/cli dev
    if ($LASTEXITCODE -ne 0) { throw "tauri dev exited with $LASTEXITCODE" }
} finally {
    Pop-Location
}
