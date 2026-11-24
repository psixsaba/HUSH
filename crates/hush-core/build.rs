fn main() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let native = manifest.join("../../native/ring");
    cc::Build::new()
        .file(native.join("ring.c"))
        .include(&native)
        .std("c11")
        .flag_if_supported("-Wall")
        .compile("hush_ring");
    println!("cargo:rerun-if-changed=../../native/ring/ring.c");
    println!("cargo:rerun-if-changed=../../native/ring/hush_ring.h");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
