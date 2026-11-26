use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).expect("copy memory.x");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("arm") {
        for name in ["__aeabi_memmove", "__aeabi_memmove4", "__aeabi_memmove8"] {
            println!("cargo:rustc-link-arg=--wrap={name}");
        }
    }
    println!(
        "cargo:rustc-link-arg=-Map={}",
        out.join("diagnostic.map").display()
    );
}
