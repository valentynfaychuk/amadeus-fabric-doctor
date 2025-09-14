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

# Installation

```bash
sudo apt-get update && sudo apt-get install -y \
    build-essential \
    cmake \
    g++ \
    libclang-dev \
    clang \
    llvm \
    libsnappy-dev \
    liblz4-dev \
    libzstd-dev \
    zlib1g-dev \
    libbz2-dev \
    pkg-config
sudo apt-get install gcc-11 g++-11
export CC=gcc-11
export CXX=g++-11
cargo clean
cargo build --release
## Features

- **Enhanced Fabric Migration**: Comprehensive migration of all Amadeus column families with height-based filtering
- **Read and Export**: List keys, get specific values, and export all contractstate data to JSON
- **Snapshot Creation**: Create deterministic, verifiable snapshots in SPK (StatePack) format
- **ETF Parsing**: Parse Erlang Term Format (ETF) data including heights and consensus state
- **Height-Based Filtering**: Intelligent migration based on temporal and rooted heights from consensus
- **Chain Traversal**: Migrate entries following blockchain structure to maintain consistency
- **Intelligent Key Decoding**: Automatically decode Base58 public keys and structured keys
- **Safe Migration**: Comprehensive error handling, validation, and verification
- **Column Family Awareness**: Proper handling of all Amadeus blockchain column families

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/amadeus-fabric-doctor`.

## Usage

### Basic Operations

#### List contractstate keys
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --list-keys
```

#### Get value for specific key
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --key <hex_encoded_key>
```

#### Export all contractstate data to JSON
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --export output.json
```

#### Create deterministic snapshot of contractstate
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --snapshot /path/to/backup.spk
```

#### Test entry hash verification
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --test verification-results.json
```

### Entry Hash Verification Testing

The `--test` command validates that entries in the database have correct hashes by recomputing them using the same algorithm as the Amadeus node.

#### How It Works
1. **Extract Sample Entries**: Selects up to 5 random entries from the `default` column family
2. **Parse Entry Structure**: Attempts to parse ETF-encoded entry data to extract stored hash
3. **Recompute Hash**: Uses the Blake3 algorithm to recompute the entry hash from header data
4. **Compare Hashes**: Validates that stored hash matches computed hash
5. **Generate Report**: Outputs detailed results to JSON file

#### Test Output Format
```json
{
  "metadata": {
    "test_time": "2025-01-15T10:30:45.123Z",
    "entries_tested": 5,
    "hash_matches": 4,
    "success_rate_percent": 80.0,
    "max_entries_tested": 5
  },
  "test_results": [
    {
      "entry_number": 1,
      "entry_key": "hexadecimal_key",
      "stored_hash": "stored_hash_hex",
      "computed_hash": "computed_hash_hex",
      "hash_matches": true,
      "entry_size_bytes": 1024
    }
  ]
}
```

#### Use Cases
- **Database Integrity**: Verify that entry hashes haven't been corrupted
- **Migration Validation**: Ensure hash computation works correctly after migration
- **Algorithm Verification**: Confirm the Rust implementation matches the Elixir node logic
- **Debugging**: Identify specific entries with hash mismatches

### Snapshot Creation

Create deterministic, verifiable snapshots of the contractstate column family using the SPK (StatePack) format.

#### Create snapshot
```bash
./amadeus-fabric-doctor --db-path /path/to/fabric/db --snapshot /backup/contractstate-snapshot
```

This will create:
- `contractstate-snapshot.spk` - Binary snapshot file
- `contractstate-snapshot.spk.manifest.json` - Metadata and verification information

#### Snapshot Features
- **Deterministic**: Same database state produces identical snapshots
- **Verifiable**: Blake3 hash ensures data integrity
- **Efficient**: Binary format with varint encoding
- **Complete**: Captures all key-value pairs in exact order
- **Portable**: Can be used for backup, migration, or analysis

#### Snapshot Format (SPK)
Follows the StatePack v1 specification:
- Magic bytes: `SPK1`
- Column family name (varint length + UTF-8 name)
- Records: varint(key_len) + key + varint(value_len) + value
- Blake3 hash of entire content for verification

### Database Migration

#### Enhanced Amadeus Fabric Migration
```bash
./amadeus-fabric-doctor --db-path /path/to/old/db --migrate /path/to/new/db
```

This enhanced migration command performs the following:

**Column Family Migration Strategy:**
1. **contractstate** (Full Migration) - All entries migrated completely
2. **sysconf** (Full Migration) - All entries migrated completely
3. **default** (Height-Filtered Migration) - Entries between temporal and rooted heights + chain traversal to genesis/gap
4. **entry_by_height** (Height-Filtered Migration) - Only entries within height range based on parsed height keys
5. **entry_by_slot** (Height-Filtered Migration) - Entries filtered by corresponding entry availability in default CF

6. **muts_rev** (Height-Filtered Migration) - Only entries between temporal and rooted heights

**Migration Process:**
1. **Extract Heights** - Reads temporal_height and rooted_tip from sysconf column family
2. **Parse ETF Terms** - Decodes Erlang Term Format data for height extraction
3. **Validate** - Ensures source database exists and contains required column families
4. **Create Target** - Creates new database with all Amadeus column families
5. **Migrate in Steps** - Processes each column family according to its migration strategy
6. **Verify** - Reports detailed statistics and migration success

#### Migration Safety Features

- **Confirmation prompt** if target database already contains data
- **Batch processing** (1000 entries per batch) for better performance
- **Size validation** (skips overly large keys/values that exceed RocksDB limits)
- **Error handling** (stops after 10 consecutive errors)
- **Verification** (compares source and target counts)
- **Progress reporting** (shows migration progress)

### Command Line Options

```
Usage: amadeus-fabric-doctor [OPTIONS] --db-path <DB_PATH>

Options:
  -d, --db-path <DB_PATH>        Path to the RocksDB database directory (source for migration)
      --migrate <TARGET_DB_PATH> Enhanced migration: contractstate+sysconf (full), default+muts_rev (height-filtered)
  -l, --list-keys                List all available keys in contractstate column family
  -k, --key <KEY>                Get value for a specific key (hex format)
  -e, --export <EXPORT>          Export all contractstate data to JSON file
  -r, --raw                      Show raw binary data (don't parse ETF)
      --snapshot <SNAPSHOT_PATH> Create snapshot of contractstate column family (SPK format)
      --test <OUTPUT_FILE>       Test entry hash verification and output results to JSON file
  -h, --help                     Print help
```

## RocksDB Version Compatibility

**CRITICAL VERSION ENFORCEMENT** - This tool now enforces proper version usage:

### Current Implementation (Fixed at 0.19.0)
- **Source Database**: Erlang-created RocksDB 7.7.3 (read with Rust crate 0.19.0/RocksDB 7.4.4)
- **Target Database**: Created with Rust crate 0.19.0/RocksDB 7.4.4 (intermediate format)
- **Compatibility**: 0.19.0 is READ-COMPATIBLE with Erlang's 7.7.3 format
- **Tool Version**: Fixed at `rocksdb = "0.19.0"` in Cargo.toml

### Two-Step Migration to Latest RocksDB:

**Step 1** (Current Tool):
```bash
# Uses rocksdb = "0.19.0"
./amadeus-fabric-doctor --db-path /erlang/db --migrate /intermediate/db
```

**Step 2** (Manual):
1. Edit `Cargo.toml`: Change `rocksdb = "0.19.0"` to `rocksdb = "0.23.0"`
2. Rebuild: `cargo build --release`
3. Run final migration:
```bash
# Now uses rocksdb = "0.23.0" (RocksDB 10.4.2+)
./amadeus-fabric-doctor --db-path /intermediate/db --migrate /final/db
```

### Why This Two-Step Process?
- **Rust limitation**: Cannot have two different RocksDB versions in one binary
- **Safety**: Ensures data integrity through intermediate compatible format
- **Verification**: Each step can be validated independently
- **Version enforcement**: Prevents accidental version mismatches

## Architecture Details

### Column Families

The tool creates and works with all Amadeus fabric database column families:
- `default`
- `entry_by_height|height:entryhash`
- `entry_by_slot|slot:entryhash`
- `tx|txhash:entryhash`
- `tx_account_nonce|account:nonce->txhash`
- `tx_receiver_nonce|receiver:nonce->txhash`
- `my_seen_time_entry|entryhash`
- `my_attestation_for_entry|entryhash`
- `consensus`
- `consensus_by_entryhash|Map<mutationshash,consensus>`
- **`contractstate`** ← Migration target
- `muts`
- `muts_rev`
- `sysconf`

### Data Processing

The tool handles various data formats stored in the contractstate column family:

1. **Plain strings**: Account balances, configuration values
2. **Integers**: Numeric values (parsed from string representation)
3. **ETF (Erlang Term Format)**: Binary data starting with magic byte 131
4. **Raw binary**: Any other binary data

### Migration Process

1. **Pre-migration checks**:
   - Validate source database exists and has contractstate CF
   - Check target database state
   - Prompt user if target has existing data

2. **Migration**:
   - Process entries in batches of 1000
   - Validate key/value sizes against RocksDB limits
   - Handle errors gracefully (max 10 consecutive errors)
   - Report progress every 1000 entries

3. **Post-migration verification**:
   - Count entries in source and target databases
   - Verify migration completeness
   - Report detailed statistics

## Error Handling

- **Database access errors**: Clear error messages for missing paths, permissions, etc.
- **Data validation errors**: Skip oversized keys/values with warnings
- **Migration errors**: Comprehensive error reporting with rollback safety
- **Verification failures**: Detailed mismatch reporting

## Performance Considerations

- **Batch processing**: 1000 entries per RocksDB write batch
- **Memory efficient**: Processes data in streaming fashion
- **Progress reporting**: Regular updates during long migrations
- **Interrupt safety**: Can be stopped safely between batches

## Examples

### Complete Migration Workflow with Snapshots

```bash
# 1. Create verifiable snapshot of original database
./amadeus-fabric-doctor --db-path /amadeus/work_folder/db/fabric --snapshot /backup/original-state

# 2. Check source database
./amadeus-fabric-doctor --db-path /amadeus/work_folder/db/fabric --list-keys

# 3. Perform migration
./amadeus-fabric-doctor --db-path /amadeus/work_folder/db/fabric --migrate /amadeus/migrated_fabric

# 4. Create snapshot of migrated database for verification
./amadeus-fabric-doctor --db-path /amadeus/migrated_fabric --snapshot /backup/migrated-state

# 5. Verify both snapshots have same content (different format, same data)
./amadeus-fabric-doctor --db-path /amadeus/migrated_fabric --list-keys
```

### Backup and Recovery Workflow

```bash
# Regular backup
./amadeus-fabric-doctor --db-path /amadeus/work_folder/db/fabric --snapshot /backups/daily-$(date +%Y%m%d)

# Verify backup integrity
sha256sum /backups/daily-20250912.spk
# Compare with manifest hash
cat /backups/daily-20250912.spk.manifest.json | jq .root_hex

# Export human-readable version for analysis
./amadeus-fabric-doctor --db-path /amadeus/work_folder/db/fabric --export /backups/daily-$(date +%Y%m%d).json
```

### Troubleshooting

#### Migration fails with "Database path does not exist"
```bash
# Check if the path exists and has correct permissions
ls -la /path/to/fabric/db
```

#### Target database already has data
The tool will prompt you to continue or cancel. Choose 'y' to merge data or 'N' to cancel.

#### Migration verification fails
Check the detailed error message. This usually indicates:
- Disk space issues
- Permission problems
- RocksDB compatibility issues

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

## Intelligent Key Decoding

**NEW FEATURE**: The tool now automatically decodes contractstate keys for human-readable output.

### Key Structure Recognition

The tool recognizes and decodes these key patterns:

1. **Coin Balances**: `bic:coin:balance:{BASE58_PUBLIC_KEY}:{SYMBOL}`
   - Example: `bic:coin:balance:8X9Ms2b4LMjFGRNj6WzxCBWbG6vXPjqwGJZzG8QDJFVi:AMA`

2. **Base Nonces**: `bic:base:nonce:{BASE58_PUBLIC_KEY}`
   - Example: `bic:base:nonce:8X9Ms2b4LMjFGRNj6WzxCBWbG6vXPjqwGJZzG8QDJFVi`

3. **Epoch Data**: `bic:epoch:trainers:height:{12_DIGIT_HEIGHT}`
   - Example: `bic:epoch:trainers:height:000000319557`

4. **Proof of Possession**: `bic:epoch:pop:{BASE58_PUBLIC_KEY}`

5. **Contract Bytecode**: `bic:contract:account:{BASE58_PUBLIC_KEY}:bytecode`

6. **Solutions Count**: `bic:epoch:solutions_count:{BASE58_PUBLIC_KEY}`

### Key Component Decoding

- **48-byte Public Keys**: Automatically converted from binary to Base58 encoding
- **Height Values**: 12-digit zero-padded numbers (e.g., `000000319557`)
- **Nonce Values**: 20-digit zero-padded numbers
- **Text Prefixes**: Preserved as human-readable strings
- **Symbols**: Cryptocurrency symbols like `AMA`, `BTC`, etc.

### Fallback Handling

- Mixed binary/text keys that don't match patterns are shown as `hex:...`
- Unknown binary sections are displayed as `:hex:{hex_data}`
- Pure UTF-8 keys are displayed as-is

## Example Output

### With Key Decoding (Default)
```json
{
  "bic:coin:balance:8X9Ms2b4LMjFGRNj6WzxCBWbG6vXPjqwGJZzG8QDJFVi:AMA": "1000000000",
  "bic:base:nonce:7YzKGqvyTFgkKPuJYRxKrS9M8tHgW8QDJFVi": "42",
  "bic:epoch:trainers:height:000000319557": {
    "etf_format": true,
    "etf_type": "LIST",
    "data_size": 145,
    "raw_hex": "..."
  },
  "bic:epoch:segment_vr_hash": "abcdef1234567890...",
  "bic:epoch:pop:9ZxKGqvyTFgkKPuJYRxKrS9M8tHgW8QDJFVi": "base64_signature_data"
}
```

### Key Listing Format
```
Keys in contractstate column family:
No.  Decoded Key                                 Raw Hex
--------------------------------------------------------------------------------
0    bic:coin:balance:8X9Ms2b4LMjFGRNj6WzxCBWb... 626963636f696e62616c...
1    bic:epoch:trainers:height:000000319557       6269633a65706f63683a7...
2    bic:base:nonce:7YzKGqvyTFgkKPuJYRxKrS9M8t... 626963626173656e6f6e...
```

### Export Structure with Metadata
```json
{
  "metadata": {
    "total_entries": 1234,
    "failed_parses": 0,
    "export_time": "2025-01-15T10:30:45Z",
    "raw_mode": false,
    "key_decoding": "enabled"
  },
  "data": {
    "bic:coin:balance:8X9Ms2b4LMjFGRNj6WzxCBWbG6vXPjqwGJZzG8QDJFVi:AMA": "1000000000",
    "bic:epoch:trainers:height:000000319557": {
      "etf_format": true,
      "etf_type": "LIST",
      "data_size": 145
    }
  }
}
```

## Contributing

This tool was built for the Amadeus blockchain project's database migration needs. It follows Rust best practices for:

- Error handling with `anyhow`
- CLI parsing with `clap`
- Safe RocksDB operations
- Memory-efficient data processing

## License

This project follows the Amadeus blockchain project licensing.