// Put onnxruntime.dll (silence trimming) next to voxa-desktop.exe.
fn main() {
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
