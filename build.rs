use std::{env, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=PORTAL_NATIVE_DIR");
    let dir = env::var_os("PORTAL_NATIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native"));
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=portal_media");
}
