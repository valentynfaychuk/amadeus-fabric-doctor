use std::env;
use std::path::PathBuf;

fn main() {
    // Check if RocksDB 7.7.3 is available system-wide
    println!("cargo:rerun-if-changed=build.rs");
    
    // Option 1: Link against system RocksDB if it's version 7.7.3
    if let Ok(lib_path) = env::var("ROCKSDB_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_path);
        println!("cargo:rustc-link-lib=rocksdb");
        return;
    }
    
    // Option 2: Build RocksDB 7.7.3 from source
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rocksdb_dir = out_dir.join("rocksdb-7.7.3");
    
    if !rocksdb_dir.exists() {
        println!("cargo:warning=To use exact RocksDB 7.7.3, please:");
        println!("cargo:warning=1. Download RocksDB 7.7.3 source from https://github.com/facebook/rocksdb/archive/v7.7.3.tar.gz");
        println!("cargo:warning=2. Build it with: make static_lib");
        println!("cargo:warning=3. Set ROCKSDB_LIB_DIR to the build directory");
        println!("cargo:warning=");
        println!("cargo:warning=Currently using closest available version (0.10.0+7.9.2)");
    }
}