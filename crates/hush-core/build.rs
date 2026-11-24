fn main() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let native = manifest.join("../../native/ring");
    let mut build = cc::Build::new();
    build
        .file(native.join("ring.c"))
        .include(&native)
        .flag_if_supported("-Wall");
    if build.get_compiler().is_like_msvc() {
        // /std:c11 alone includes vcruntime_c11_stdatomic.h, which errors
        // unless the experimental C11 atomics switch is also on. _Atomic in
        // ring.c needs that pair.
        build.flag("/std:c11");
        build.flag("/experimental:c11atomics");
    } else {
        build.std("c11");
    }
    build.compile("hush_ring");
    println!("cargo:rerun-if-changed=../../native/ring/ring.c");
    println!("cargo:rerun-if-changed=../../native/ring/hush_ring.h");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
