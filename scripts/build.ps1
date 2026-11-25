param(
    [switch]$SkipTests,
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $RepoRoot

function Assert-ExitOk([string]$StepName) {
    if ($LASTEXITCODE -ne 0) {
        throw "$StepName failed with exit code $LASTEXITCODE"
    }
}

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
if ($Offline) {
    $env:CARGO_NET_OFFLINE = "true"
}
$env:PATH = (($ExtraPaths | Where-Object { Test-Path $_ }) -join ";") + ";" + $env:PATH

try {
    & (Join-Path $PSScriptRoot "fetch-ffmpeg.ps1")

    # Ensure Tauri sidecar copies in target/{debug,release} never go stale when binaries/ changes
    foreach ($profile in @("debug", "release")) {
        $srcFfmpeg = Join-Path $RepoRoot "src-tauri\binaries\ffmpeg-x86_64-pc-windows-msvc.exe"
        $srcFfprobe = Join-Path $RepoRoot "src-tauri\binaries\ffprobe-x86_64-pc-windows-msvc.exe"
        $dstDir = Join-Path $RepoRoot "target\$profile"
        if (Test-Path $dstDir) {
            Copy-Item -Force $srcFfmpeg (Join-Path $dstDir "ffmpeg.exe")
            Copy-Item -Force $srcFfprobe (Join-Path $dstDir "ffprobe.exe")
        }
    }

    if (-not $SkipTests) {
        Write-Host "Running frontend strict typecheck & build..."
        npm run typecheck; Assert-ExitOk "npm run typecheck"
        npm run build; Assert-ExitOk "npm run build"

        Write-Host "Running Rust checks, clippy, and tests..."
        cargo check --workspace --all-targets; Assert-ExitOk "cargo check"
        cargo clippy --workspace --all-targets -- -D warnings; Assert-ExitOk "cargo clippy"
        cargo test --workspace; Assert-ExitOk "cargo test"
    }

    Write-Host "Building standalone release .exe and NSIS installer .exe..."
    npx --yes @tauri-apps/cli build; Assert-ExitOk "tauri build"
} finally {
    Pop-Location
}
