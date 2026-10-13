fn main() {
    // memory.x 所在目录（无论从哪里调用 cargo 都正确）。
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={}", manifest_dir);
    println!("cargo:rerun-if-changed=memory.x");
}
