# Amadeus Fabric Doctor

CLI tool for reading, migrating, and analyzing Amadeus blockchain fabric databases (RocksDB format).

## Quick Start

```bash
cargo build --release
./target/release/amadeus-fabric-doctor --help
```

## Where do I get a source database?

The rocksdb database is snapshotted regularly and available at:
```bash
wget https://snapshots.amadeus.bot/000034076355.zip
```

## What Can I Do?

### List Keys in ContractState
**Want to**: See what's in your database
**Run**: `./amadeus-fabric-doctor --db-path /path/to/db --list-keys`
**Result**: Shows up to 100 keys with decoded names (balances, nonces, etc.)

### Get Specific Value
**Want to**: Look up a single key's value
**Run**: `./amadeus-fabric-doctor --db-path /path/to/db --key <hex_key>`
**Result**: Displays the value, parsed as JSON if it's ETF format

### Export All Data
**Want to**: Dump entire contractstate to JSON file
**Run**: `./amadeus-fabric-doctor --db-path /path/to/db --export output.json`
**Result**: Creates JSON file with all key-value pairs and metadata

### Migrate Full Database
**Want to**: Migrate all critical data with smart filtering
**Run**: `./amadeus-fabric-doctor --db-path /source/db --migrate /target/db`
**Result**: Migrates:
- **contractstate**: Full (all 11k+ kvs)
- **sysconf**: Full + adds rooted_height
- **default**: Smart filtered (temporal→rooted entries + chain to genesis)
- **muts_rev, muts, my_attestations, consensus**: Filtered (only temporal entries)

### Migrate Only State (Quick)
**Want to**: Just migrate contractstate + sysconf (fastest migration)
**Run**: `./amadeus-fabric-doctor --db-path /source/db --weakmigrate /target/db`
**Result**: Migrates only:
- **contractstate**: Full (all 11k+ kvs)
- **sysconf**: Full (3 kvs)
- Skips blockchain entries, indexes, and muts_rev

### Create Snapshot
**Want to**: Verifiable backup of contractstate
**Run**: `./amadeus-fabric-doctor --db-path /path/to/db --snapshot /backup/state`
**Result**: Creates `state.spk` (binary) and `state.spk.manifest.json` (metadata with hash)

### Test Entry Hashes
**Want to**: Verify blockchain entry integrity
**Run**: `./amadeus-fabric-doctor --db-path /path/to/db --test results.json`
**Result**: Tests 5 entries, outputs hash verification results to JSON

## When to Use What?

| Scenario | Command | Why |
|----------|---------|-----|
| Quick inspection | `--list-keys` | See what data exists |
| Backup before upgrade | `--snapshot` | Create verifiable backup |
| **Migrate just state** | `--weakmigrate` | Fastest, only contract+system state |
| **Migrate everything** | `--migrate` | Complete migration with chain data |
| Verify after migration | `--test` | Check entry hashes are correct |
| Debug specific issue | `--key` + `--export` | Examine exact data |

## Migration Comparison

| Feature | --weakmigrate (Fast) | --migrate (Complete) |
|---------|---------------------|----------------------|
| contractstate | ✅ Full | ✅ Full |
| sysconf | ✅ Full | ✅ Full + rooted_height |
| Blockchain entries | ❌ Skipped | ✅ Filtered by height |
| Entry indexes | ❌ Skipped | ✅ Filtered by height |
| muts_rev, muts, my_attestations, consensus | ❌ Skipped | ✅ Temporal entries only |
| **Speed** | **Fast** | Slower (more data) |
| **Use for** | **State-only needs** | Full node operation |

## Common Workflows

### Fast State Migration
```bash
# Best for: Moving just the smart contract state
./amadeus-fabric-doctor --db-path /old/db --weakmigrate /new/db
```

### Full Database Migration
```bash
# Best for: Complete node migration with blockchain data
./amadeus-fabric-doctor --db-path /old/db --migrate /new/db
```

### Backup & Verify
```bash
# Create backup
./amadeus-fabric-doctor --db-path /prod/db --snapshot /backup/state-$(date +%Y%m%d)

# Verify integrity (compare hashes)
cat /backup/state-*.spk.manifest.json | jq .root_hex
```

### Before/After Migration Check
```bash
# Before
./amadeus-fabric-doctor --db-path /old/db --list-keys | head -20

# Migrate
./amadeus-fabric-doctor --db-path /old/db --weakmigrate /new/db

# After
./amadeus-fabric-doctor --db-path /new/db --list-keys | head -20
```

## Key Decoding

Keys are automatically decoded for readability:

| Pattern | Example |
|---------|---------|
| Balances | `bic:coin:balance:{Base58PubKey}:AMA` |
| Nonces | `bic:base:nonce:{Base58PubKey}` |
| Epoch data | `bic:epoch:trainers:height:000000319557` |
| Contract code | `bic:contract:account:{Base58PubKey}:bytecode` |

## Installation Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get install -y build-essential cmake g++ libclang-dev clang \
  libsnappy-dev liblz4-dev libzstd-dev zlib1g-dev libbz2-dev pkg-config
cargo build --release
```

**macOS:**
```bash
brew install cmake llvm snappy lz4 zstd
cargo build --release
```

## Column Families

| CF Name | Contains | Migrated by --weakmigrate? |
|---------|----------|---------------------------|
| contractstate | Smart contract state | ✅ Yes |
| sysconf | System config (heights, tips) | ✅ Yes |
| default | Blockchain entries | ❌ No (--migrate only) |
| entry_by_height | Height→entry index | ❌ No |
| entry_by_slot | Slot→entry index | ❌ No |
| muts_rev | Mutation reverse lookup | ❌ No |
| Others | TX indexes, consensus, etc. | ❌ No |

## Troubleshooting

**"Database path does not exist"**
→ Check path and permissions: `ls -la /path/to/db`

**"Target database already contains data"**
→ Tool prompts to continue (y) or cancel (N)

**Migration verification fails**
→ Check disk space, permissions, or RocksDB compatibility

**Slow migration**
→ Use `--weakmigrate` if you only need state, not full blockchain history

## Options Reference

```
  -d, --db-path <DB_PATH>             Source database path
      --migrate <TARGET_DB_PATH>      Full migration (state + filtered blockchain)
      --weakmigrate <TARGET_DB_PATH>  Fast migration (state only)
  -l, --list-keys                     List contractstate keys
  -k, --key <KEY>                     Get specific key value (hex)
  -e, --export <FILE>                 Export all data to JSON
      --snapshot <PATH>               Create SPK snapshot
      --test <FILE>                   Test entry hash verification
  -r, --raw                           Show raw binary (no parsing)
  -h, --help                          Show help
```

## RocksDB Version Note

Tool uses RocksDB 7.4.4 (close to Erlang's 7.7.3). For production migrations to latest RocksDB 10.x, use two-step process (see old README for details).
