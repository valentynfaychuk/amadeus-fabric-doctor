fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if let Ok(lib_path) = std::env::var("ROCKSDB_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_path);
        println!("cargo:rustc-link-lib=rocksdb");
    }
}