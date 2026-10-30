fn main() {
    println!("cargo:rerun-if-changed=assets/reference/ico/MAINICON.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let icon = root.join("assets/reference/ico/MAINICON.ico");
    let rc_source = output.join("anycompare.rc");
    let resource = output.join("anycompare.res");
    let source = format!(
        "1 ICON \"{}\"\n",
        icon.display().to_string().replace('\\', "\\\\")
    );
    std::fs::write(&rc_source, source).unwrap();
    let kits = std::path::Path::new("C:/Program Files (x86)/Windows Kits/10/bin");
    let mut versions: Vec<_> = std::fs::read_dir(kits)
        .expect("Windows SDK is required to build Windows resources")
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64/rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    versions.sort();
    let compiler = versions
        .last()
        .expect("Windows SDK resource compiler rc.exe was not found");
    let result = std::process::Command::new(compiler)
        .args(["/nologo", "/fo"])
        .arg(&resource)
        .arg(&rc_source)
        .output()
        .expect("Cannot start Windows resource compiler");
    assert!(
        result.status.success(),
        "Windows resource compilation failed: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("cargo:rustc-link-arg={}", resource.display());
}
