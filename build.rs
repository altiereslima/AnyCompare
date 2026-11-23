fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let icon = root.join("assets/icons/app.ico");
    let rc_source = output.join("anycompare.rc");
    let resource = output.join("anycompare.res");
    let pkg_ver = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.2.6".to_string());
    let parts: Vec<u16> = pkg_ver
        .split('.')
        .filter_map(|p| p.parse::<u16>().ok())
        .collect();
    let (maj, min, pat) = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(2),
        parts.get(2).copied().unwrap_or(6),
    );
    let source = format!(
        "1 ICON \"{}\"\n\
1 VERSIONINFO\n\
FILEVERSION {maj},{min},{pat},0\n\
PRODUCTVERSION {maj},{min},{pat},0\n\
FILEOS 0x40004L\n\
FILETYPE 0x1L\n\
BEGIN\n\
  BLOCK \"StringFileInfo\"\n\
  BEGIN\n\
    BLOCK \"040904B0\"\n\
    BEGIN\n\
      VALUE \"CompanyName\", \"AnyCompare\\0\"\n\
      VALUE \"FileDescription\", \"AnyCompare - File & Folder Comparison\\0\"\n\
      VALUE \"FileVersion\", \"{pkg_ver}.0\\0\"\n\
      VALUE \"InternalName\", \"anycompare\\0\"\n\
      VALUE \"OriginalFilename\", \"anycompare.exe\\0\"\n\
      VALUE \"ProductName\", \"AnyCompare\\0\"\n\
      VALUE \"ProductVersion\", \"{pkg_ver}\\0\"\n\
    END\n\
  END\n\
  BLOCK \"VarFileInfo\"\n\
  BEGIN\n\
    VALUE \"Translation\", 0x0409, 1200\n\
  END\n\
END\n",
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
