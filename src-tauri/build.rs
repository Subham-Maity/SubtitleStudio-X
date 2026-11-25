fn main() {
    println!("cargo:rerun-if-changed=binaries");
    println!("cargo:rerun-if-changed=binaries/ffmpeg-x86_64-pc-windows-msvc.exe");
    println!("cargo:rerun-if-changed=binaries/ffprobe-x86_64-pc-windows-msvc.exe");

    // Delay-load vulkan-1.dll on Windows MSVC so the binary can launch cleanly on
    // GPU-less machines or VMs without Vulkan drivers and fall back to CPU Whisper.
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        println!("cargo:rustc-link-arg=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }

    tauri_build::build();
}
