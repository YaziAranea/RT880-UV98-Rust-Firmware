use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("link.x"), include_bytes!("link.x")).expect("copy link.x");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=link.x");
    println!("cargo:rerun-if-changed=src/probe.s");
    println!(
        "cargo:rustc-link-arg=-Map={}",
        out.join("uart-probe.map").display()
    );
}
