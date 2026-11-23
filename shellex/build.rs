fn main() {
    println!("cargo:rerun-if-changed=exports.def");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let def = std::path::Path::new(&dir).join("exports.def");
        println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
    }
}
