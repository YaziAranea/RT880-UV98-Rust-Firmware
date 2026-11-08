fn main() {
    println!("cargo:rerun-if-env-changed=TARGET");
    if std::env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnullvm") {
        let imports =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/windows-imports");
        println!("cargo:rustc-link-search=native={}", imports.display());
    }
}
