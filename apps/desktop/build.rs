// Put onnxruntime.dll (silence trimming) next to voxa-desktop.exe, and embed the app icon
// (resource 1, which GPUI uses for its windows; Explorer and the taskbar use it too).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "Voxa");
        res.set("ProductName", "Voxa");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return; // the ONNX runtime is bundled by the macOS packaging script
    }

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/onnxruntime/onnxruntime.dll");
    println!("cargo:rerun-if-changed={}", src.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // OUT_DIR is <profile>/build/<pkg>[/<hash>]/out (layout varies by cargo version): the profile
    // dir is the parent of the nearest "build" ancestor.
    let profile_dir = out.ancestors().find(|a| a.file_name().is_some_and(|n| n == "build")).and_then(|b| b.parent()).unwrap().to_path_buf();
    if src.exists() {
        for dir in [profile_dir.clone(), profile_dir.join("examples"), profile_dir.join("deps")] {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::copy(&src, dir.join("onnxruntime.dll"));
        }
    } else {
        println!("cargo:warning=vendor/onnxruntime/onnxruntime.dll missing: run scripts/fetch-onnxruntime.ps1 (long recordings will be sent untrimmed)");
    }
}
