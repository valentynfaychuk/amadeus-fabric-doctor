# Amadeus Fabric Doctor

The doctor can work with the Amadeus Fabric folder (just like the node itself and
perform some high complexity custom data migration, for example to the newer version
of RocksDB that is used by Rust. Technically, this is a Rust CLI tool for reading
and analyzing the Amadeus blockchain fabric database stored in RocksDB format.

The current RocksDB version mismatch is:

- Erlang rust wrapper: RocksDB 7.7.3 (October 2022), 1.6.0-69-g18add8e9 based on erlang-rocksdb version 1.6.0
- Rust rocksdb crate 0.24.0: RocksDB 10.4.2 (much newer), librocksdb-sys v0.17.3+10.4.2

Because no rust crate has exactly 7.7.3, the repository uses the best effort 7.4.4
which should work fine for the purposes of this tool. In case there are any compatibility
issues, `Cargo_exact.toml` shows options to get exactly RocksDB 7.7.3, with a bit of
a headache.

## Features

- **Database Discovery**: Automatically detects column families in the RocksDB database
- **ETF Parsing**: Parses Erlang Term Format (ETF) data into human-readable JSON
- **Key Listing**: List all keys in the `contractstate` column family
- **Value Retrieval**: Get specific values by key (hex format)
- **Data Export**: Export all contractstate data to JSON file
- **Raw Mode**: Option to view raw binary data without ETF parsing

## Installation

```bash
cd fabric-reader
cargo build --release
```

## Usage

### List all keys in contractstate column family
```bash
./target/release/fabric-reader --db-path /path/to/fabric/db --list-keys
```

### Get a specific value by key (hex format)
```bash
./target/release/fabric-reader --db-path /path/to/fabric/db --key "deadbeef123456"
```

### Export all contractstate data to JSON
```bash
./target/release/fabric-reader --db-path /path/to/fabric/db --export output.json
```

### View raw binary data (without ETF parsing)
```bash
./target/release/fabric-reader --db-path /path/to/fabric/db --key "deadbeef123456" --raw
```

## RocksDB Version Compatibility

**CRITICAL VERSION INFORMATION**:
- **Erlang wrapper uses**: RocksDB **7.7.3** (verified in `/deps/rocksdb/include/rocksdb/version.h`)
- **Rust tool uses**: RocksDB **7.4.4** via rocksdb crate 0.19.0 (librocksdb-sys 0.8.3+7.4.4)

### Why Not Exact Version Match?
- **No Rust crate exists** with RocksDB 7.7.3 exactly
- Closest available versions:
  - 0.8.3+7.4.4 (older, what we use)
  - 0.10.0+7.9.2 (newer)

### Safety Considerations:
- Using **7.4.4 to read 7.7.3** database is generally safe for reading
- RocksDB maintains backward compatibility for reading
- **This tool is READ-ONLY** - no writes to prevent any compatibility issues
- For exact version matching, see `README_EXACT_VERSION.md`

## Column Families

The tool automatically discovers column families but has built-in fallbacks for known Amadeus column families:
- `default`
- `contractstate` (main focus of this tool)
- `bic:epoch:trainers`
- `consensus`

## Data Format Support

The tool intelligently parses various data formats found in the contractstate column family:

### String Values
- Plain UTF-8 strings are returned as JSON strings
- Numeric strings are automatically parsed as integers
- Examples: `"1000000"` → `1000000`, `"bic:coin:balance"` → `"bic:coin:balance"`

### ETF (Erlang Term Format) Detection
When ETF binary data is detected (starts with magic byte 131), the tool provides:
- ETF format identification
- Type byte analysis (ATOM, INTEGER, TUPLE, LIST, MAP, etc.)
- Raw hex data for further analysis
- Data size information

### Binary Data
- Non-UTF-8 binary data is returned as hex-encoded strings
- Includes size information for analysis

## Example Output

### String/Integer Values
```json
{
  "626963": "bic:coin:balance:somepubkey:AMA",
  "1000000": 1000000,
  "somekey": "trainer_public_key_hex"
}
```

### ETF Format Detection
```json
{
  "etf_encoded_key": {
    "etf_format": true,
    "magic_byte": 131,
    "etf_type": "LIST",
    "etf_type_byte": 108,
    "data_size": 45,
    "raw_hex": "836c00000003..."
  }
}
```

### Export Structure
```json
{
  "metadata": {
    "total_entries": 1234,
    "failed_parses": 0,
    "export_time": "2024-01-15T10:30:45Z",
    "raw_mode": false
  },
  "data": {
    "hex_key_1": "string_value",
    "hex_key_2": 1000000,
    "hex_key_3": {
      "etf_format": true,
      "etf_type": "TUPLE",
      "raw_hex": "..."
    }
  }
}
```