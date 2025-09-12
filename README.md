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

- **Read and Export**: List keys, get specific values, and export all contractstate data to JSON
- **Database Migration**: Migrate contractstate column family from RocksDB 7.4.4/7.7.3 to RocksDB 10.4.2+
- **ETF Parsing**: Basic parsing of Erlang Term Format (ETF) data stored in the database
- **Safe Migration**: Comprehensive error handling, validation, and verification

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/fabric-reader`.

## Usage

### Basic Operations

#### List contractstate keys
```bash
./fabric-reader --db-path /path/to/fabric/db --list-keys
```

#### Get value for specific key
```bash
./fabric-reader --db-path /path/to/fabric/db --key <hex_encoded_key>
```

#### Export all contractstate data to JSON
```bash
./fabric-reader --db-path /path/to/fabric/db --export output.json
```

### Database Migration

#### Migrate contractstate from old RocksDB to new RocksDB
```bash
./fabric-reader --db-path /path/to/old/db --db2-path /path/to/new/db --migrate
```

This command will:
1. **Validate** that the source database exists and contains the contractstate column family
2. **Create** a new database at `db2-path` if it doesn't exist (with latest RocksDB 10.4.2+)
3. **Migrate** all contractstate data from the source to the target database
4. **Verify** that the migration was successful by comparing entry counts
5. **Report** detailed statistics about the migration

#### Migration Safety Features

- **Confirmation prompt** if target database already contains data
- **Batch processing** (1000 entries per batch) for better performance
- **Size validation** (skips overly large keys/values that exceed RocksDB limits)
- **Error handling** (stops after 10 consecutive errors)
- **Verification** (compares source and target counts)
- **Progress reporting** (shows migration progress)

### Command Line Options

```
Usage: fabric-reader [OPTIONS] --db-path <DB_PATH>

Options:
  -d, --db-path <DB_PATH>    Path to the RocksDB database directory (source for migration)
      --db2-path <DB2_PATH>  Path to the second RocksDB database directory (target for migration)
      --migrate              Migrate contractstate column family from db-path to db2-path
  -l, --list-keys            List all available keys in contractstate column family
  -k, --key <KEY>            Get value for a specific key (hex format)
  -e, --export <EXPORT>      Export all contractstate data to JSON file
  -r, --raw                  Show raw binary data (don't parse ETF)
  -h, --help                 Print help
```

## RocksDB Version Compatibility

### Source Database (--db-path)
- **Erlang RocksDB**: 7.7.3 (as used by Amadeus blockchain node)
- **Rust Crate**: 0.19.0 (closest available, bundles ~7.4.4)

### Target Database (--db2-path)
- **RocksDB**: 10.4.2+
- **Rust Crate**: 0.23.0 (latest stable, bundles 10.4.2)

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

### Complete Migration Workflow

```bash
# 1. Check source database
./fabric-reader --db-path /amadeus/work_folder/db/fabric --list-keys

# 2. Export current state (optional backup)
./fabric-reader --db-path /amadeus/work_folder/db/fabric --export backup.json

# 3. Perform migration
./fabric-reader --db-path /amadeus/work_folder/db/fabric --db2-path /amadeus/migrated_fabric --migrate

# 4. Verify migrated database
./fabric-reader --db-path /amadeus/migrated_fabric --list-keys
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