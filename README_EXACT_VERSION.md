# Exact RocksDB Version Matching

## Problem
The Amadeus Erlang node uses **RocksDB 7.7.3** exactly, but no published Rust crate bundles this exact version.

## Available Rust Crate Versions
After extensive search through crates.io and GitHub:
- **librocksdb-sys 0.8.3+7.4.4**: RocksDB 7.4.4 (older than needed)
- **librocksdb-sys 0.10.0+7.9.2**: RocksDB 7.9.2 (newer than needed)
- **No crate with 7.7.3 exists**

## Solutions

### Option 1: Use Closest Version (NOT RECOMMENDED)
```toml
# In Cargo.toml - Uses RocksDB 7.9.2
rocksdb = "0.20.0"  
```
**Risk**: Format changes between 7.7.3 and 7.9.2 could cause data corruption.

### Option 2: Build Custom Binding (RECOMMENDED)
1. Clone RocksDB 7.7.3 source:
```bash
git clone --branch v7.7.3 --depth 1 https://github.com/facebook/rocksdb.git
cd rocksdb
make static_lib -j8
```

2. Build fabric-reader with system library:
```bash
export ROCKSDB_LIB_DIR=/path/to/rocksdb
export ROCKSDB_INCLUDE_DIR=/path/to/rocksdb/include
cargo build --release
```

### Option 3: Use C FFI Directly
Create minimal C bindings to RocksDB 7.7.3:
```rust
// Use bindgen to generate bindings from rocksdb/include/rocksdb/c.h
#[link(name = "rocksdb")]
extern "C" {
    // Minimal functions needed for read-only access
    fn rocksdb_open_for_read_only(...);
    fn rocksdb_get(...);
    // etc.
}
```

### Option 4: Use Erlang RocksDB from Rust
Since the Erlang node already has the correct version, we could:
1. Extract the compiled `.so`/`.dylib` from erlang-rocksdb
2. Link against it directly
3. Use unsafe FFI to call the C API

## Current Implementation Status
The fabric-reader currently uses the standard rocksdb crate which may have version incompatibilities. For production use with exact version matching, one of the above solutions must be implemented.

## Version Compatibility Matrix
| Component | Version | RocksDB | Status |
|-----------|---------|---------|---------|
| Erlang wrapper | 18add8e9 | 7.7.3 | ✅ Exact |
| rocksdb 0.19.0 | - | ~7.4.4 | ⚠️ Close |
| rocksdb 0.20.0 | - | 7.9.2 | ⚠️ Close |
| rocksdb 0.24.0 | - | 10.4.2 | ❌ Too new |

## Recommendation
For critical production use, implement Option 2 or 3 to ensure exact version compatibility. The database format can change between minor versions, and using a different version risks data corruption or inability to read certain keys.