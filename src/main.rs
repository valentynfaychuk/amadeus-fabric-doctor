use anyhow::{anyhow, Result};
use clap::Parser;
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde_json::json;
use std::path::Path;
use eetf::Term;

mod utils;

#[derive(Parser)]
#[command(name = "amadeus-fabric-doctor")]
#[command(about = "A CLI tool to read Amadeus fabric database and parse ETF terms to JSON with migration support")]
struct Cli {
    /// Path to the RocksDB database directory (source for migration)
    #[arg(short, long)]
    db_path: String,

    /// Migrate contractstate column family from db-path to this target database path
    #[arg(long, value_name = "TARGET_DB_PATH")]
    migrate: Option<String>,

    /// Migrate only contractstate and sysconf column families from db-path to this target database path
    #[arg(long, value_name = "TARGET_DB_PATH")]
    weakmigrate: Option<String>,

    /// List all available keys in contractstate column family
    #[arg(short, long)]
    list_keys: bool,

    /// Get value for a specific key (hex format)
    #[arg(short, long)]
    key: Option<String>,

    /// Export all contractstate data to JSON file
    #[arg(short, long)]
    export: Option<String>,

    /// Show raw binary data (don't parse ETF)
    #[arg(short, long)]
    raw: bool,

    /// Test entry hash verification and output results to JSON file
    #[arg(long, value_name = "OUTPUT_FILE")]
    test: Option<String>,

    /// Show temporal and rooted tips from sysconf
    #[arg(long)]
    tips: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(target_db_path) = cli.migrate {
        perform_migration(&cli.db_path, &target_db_path)?;
    } else if let Some(target_db_path) = cli.weakmigrate {
        perform_weak_migration(&cli.db_path, &target_db_path)?;
    } else {
        // Open the database with contractstate column family (read-only for inspection)
        let db = open_source_database_readonly(&cli.db_path)?;

        let contractstate_cf = db
            .cf_handle("contractstate")
            .ok_or_else(|| anyhow!("contractstate column family not found"))?;

        if cli.tips {
            show_tips(&db)?;
        } else if cli.list_keys {
            list_keys(&db, &contractstate_cf)?;
        } else if let Some(key_hex) = cli.key {
            get_value(&db, &contractstate_cf, &key_hex, cli.raw)?;
        } else if let Some(output_file) = cli.export {
            export_all_data(&db, &contractstate_cf, &output_file, cli.raw)?;
        } else if let Some(test_output_file) = cli.test {
            test_entry_hash_verification(&db, &test_output_file)?;
        } else {
            println!("Use --help to see available options");
        }
    }

    Ok(())
}

fn open_source_database_readonly(db_path: &str) -> Result<DB> {
    if !Path::new(db_path).exists() {
        return Err(anyhow!("Source database path does not exist: {}", db_path));
    }

    let mut opts = Options::default();
    opts.create_if_missing(false);

    // Try to discover existing column families
    let cf_names = match DB::list_cf(&opts, db_path) {
        Ok(names) => names,
        Err(_) => {
            // Fallback to known column families from the Amadeus codebase
            vec![
                "default".to_string(),
                "entry_by_height|height:entryhash".to_string(),
                "entry_by_slot|slot:entryhash".to_string(),
                "tx|txhash:entryhash".to_string(),
                "tx_account_nonce|account:nonce->txhash".to_string(),
                "tx_receiver_nonce|receiver:nonce->txhash".to_string(),
                "my_seen_time_entry|entryhash".to_string(),
                "my_attestation_for_entry|entryhash".to_string(),
                "consensus".to_string(),
                "consensus_by_entryhash|Map<mutationshash,consensus>".to_string(),
                "contractstate".to_string(),
                "muts".to_string(),
                "muts_rev".to_string(),
                "sysconf".to_string(),
            ]
        }
    };

    println!("Found column families: {:?}", cf_names);

    let cf_descriptors: Vec<_> = cf_names
        .iter()
        .map(|name| ColumnFamilyDescriptor::new(name, Options::default()))
        .collect();

    // CRITICAL: Open in read-only mode to prevent any writes to source database
    let db = DB::open_cf_descriptors_read_only(&opts, db_path, cf_descriptors, false)?;
    println!("🔒 Source database opened in READ-ONLY mode");
    Ok(db)
}

fn open_target_database_readwrite(db_path: &str) -> Result<DB> {
    if !Path::new(db_path).exists() {
        return Err(anyhow!("Target database path does not exist: {}", db_path));
    }

    let mut opts = Options::default();
    opts.create_if_missing(false);
    // Limit open files to prevent "Too many open files" error during large migrations
    opts.set_max_open_files(1000);

    // Try to discover existing column families
    let cf_names = match DB::list_cf(&opts, db_path) {
        Ok(names) => names,
        Err(_) => {
            // Fallback to known column families from the Amadeus codebase
            vec![
                "default".to_string(),
                "entry_by_height|height:entryhash".to_string(),
                "entry_by_slot|slot:entryhash".to_string(),
                "tx|txhash:entryhash".to_string(),
                "tx_account_nonce|account:nonce->txhash".to_string(),
                "tx_receiver_nonce|receiver:nonce->txhash".to_string(),
                "my_seen_time_entry|entryhash".to_string(),
                "my_attestation_for_entry|entryhash".to_string(),
                "consensus".to_string(),
                "consensus_by_entryhash|Map<mutationshash,consensus>".to_string(),
                "contractstate".to_string(),
                "muts".to_string(),
                "muts_rev".to_string(),
                "sysconf".to_string(),
            ]
        }
    };

    println!("Found column families: {:?}", cf_names);

    let cf_descriptors: Vec<_> = cf_names
        .iter()
        .map(|name| ColumnFamilyDescriptor::new(name, Options::default()))
        .collect();

    let db = DB::open_cf_descriptors(&opts, db_path, cf_descriptors)?;
    println!("📝 Target database opened in READ-WRITE mode (max_open_files=1000)");
    Ok(db)
}

fn perform_migration(source_db_path: &str, target_db_path: &str) -> Result<()> {
    println!("🔄 Starting comprehensive fabric migration from {} to {}", source_db_path, target_db_path);

    // Validate source database exists
    if !Path::new(source_db_path).exists() {
        return Err(anyhow!("Source database path does not exist: {}", source_db_path));
    }

    // Create target database if it doesn't exist
    create_target_database(target_db_path)?;

    // Open source and target databases
    println!("📖 Opening source database...");
    let source_db = open_source_database_readonly(source_db_path)?;
    println!("🎯 Opening target database...");
    let target_db = open_target_database_readwrite(target_db_path)?;

    // Step 1: Extract temporal and rooted heights from consensus
    let (temporal_height, rooted_height) = extract_heights(&source_db)?;
    println!("📊 Heights - Temporal: {}, Rooted: {}", temporal_height, rooted_height);

    // Step 2: Migrate contractstate (full)
    migrate_contractstate_full(&source_db, &target_db)?;

    // Step 3: Migrate sysconf (full) + add rooted_height
    migrate_sysconf_full(&source_db, &target_db)?;
    write_rooted_height_to_sysconf(&target_db, rooted_height)?;

    // Step 4: Migrate default CF (selective: temporal to rooted + chain to genesis)
    let migrated_entry_hashes = migrate_default_selective(&source_db, &target_db, temporal_height, rooted_height)?;

    // Step 5: Migrate muts_rev, muts, my_attestations, consensus (temporal entries only)
    migrate_muts_rev_selective(&source_db, &target_db, &migrated_entry_hashes)?;
    migrate_muts_selective(&source_db, &target_db, &migrated_entry_hashes)?;
    migrate_my_attestations_selective(&source_db, &target_db, &migrated_entry_hashes)?;
    migrate_consensus_selective(&source_db, &target_db, &migrated_entry_hashes)?;

    println!("✅ Comprehensive migration completed successfully!");
    Ok(())
}

fn perform_weak_migration(source_db_path: &str, target_db_path: &str) -> Result<()> {
    println!("🔄 Starting weak migration (contractstate + sysconf only) from {} to {}", source_db_path, target_db_path);

    // Validate source database exists
    if !Path::new(source_db_path).exists() {
        return Err(anyhow!("Source database path does not exist: {}", source_db_path));
    }

    // Create target database if it doesn't exist
    create_target_database(target_db_path)?;

    // Open source and target databases
    println!("📖 Opening source database...");
    let source_db = open_source_database_readonly(source_db_path)?;
    println!("🎯 Opening target database...");
    let target_db = open_target_database_readwrite(target_db_path)?;

    // Migrate contractstate (full)
    migrate_contractstate_full(&source_db, &target_db)?;

    // Migrate sysconf (full)
    migrate_sysconf_full(&source_db, &target_db)?;

    println!("✅ Weak migration completed successfully!");
    Ok(())
}

fn create_target_database(db_path: &str) -> Result<()> {
    let path = Path::new(db_path);

    if path.exists() {
        println!("ℹ️  Target database already exists at: {}", db_path);
        return Ok(());
    }

    println!("🏗️  Creating new database at: {}", db_path);

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    // Limit open files to prevent "Too many open files" error
    opts.set_max_open_files(1000);
    
    // Define all the column families that exist in the Amadeus fabric database (new format)
    let cf_names = vec![
        "default",
        "entry",
        "entry_by_height|height->entryhash",
        "entry_by_slot|slot->entryhash",
        "my_seen_time_entry|entryhash->ts_sec",
        "my_attestation_for_entry|entryhash->attestation",
        "tx|txhash->entryhash",
        "tx_account_nonce|account:nonce->txhash",
        "tx_receiver_nonce|receiver:nonce->txhash",
        "consensus",
        "consensus_by_entryhash|Map<mutationshash,consensus>",
        "contractstate",
        "muts",
        "muts_rev",
        "sysconf",
    ];
    
    let cf_descriptors: Vec<_> = cf_names
        .iter()
        .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
        .collect();
    
    let _db = DB::open_cf_descriptors(&opts, db_path, cf_descriptors)?;
    println!("✅ Created new database with {} column families", cf_names.len());
    
    Ok(())
}

fn extract_heights(source_db: &DB) -> Result<(u64, u64)> {
    let sysconf_cf = source_db
        .cf_handle("sysconf")
        .ok_or_else(|| anyhow!("sysconf column family not found"))?;

    // Debug: List all keys in sysconf CF to see what's actually there
    println!("🔍 Debugging sysconf CF contents:");
    let iter = source_db.iterator_cf(&sysconf_cf, rocksdb::IteratorMode::Start);
    for (i, item) in iter.enumerate() {
        if i >= 20 { // Limit to first 20 entries
            println!("... (showing first 20 entries)");
            break;
        }
        match item {
            Ok((key, value)) => {
                let key_str = std::str::from_utf8(&key).unwrap_or("<invalid UTF-8>");
                let key_hex = hex::encode(&key);
                println!("  Key: '{}' (hex: {}) -> Value: {} bytes", key_str, key_hex, value.len());
            }
            Err(e) => println!("  Error reading entry: {}", e),
        }
    }

    let mut temporal_height = 0u64;
    let mut rooted_height = 0u64;

    // Get temporal_height from sysconf CF (stored as ETF-encoded term with string key)
    println!("🔍 Looking for temporal_height...");
    if let Some(value) = source_db.get_cf(&sysconf_cf, "temporal_height".as_bytes())? {
        println!("  Found temporal_height: {} bytes", value.len());
        // Parse ETF-encoded height value
        if let Ok(term) = Term::decode(&value[..]) {
            if let Term::BigInteger(big_int) = term {
                temporal_height = big_int.value.clone().try_into().unwrap_or(0);
                println!("  Parsed temporal_height: {}", temporal_height);
            } else if let Term::FixInteger(fix_int) = term {
                temporal_height = fix_int.value as u64;
                println!("  Parsed temporal_height (small int): {}", temporal_height);
            } else {
                println!("  temporal_height is not an integer: {:?}", term);
            }
        } else {
            println!("  Failed to decode temporal_height as ETF");
        }
    } else {
        return Err(anyhow!("temporal_height not found in sysconf CF"));
    }

    // Get rooted_height by looking up rooted_tip hash and getting the entry
    println!("🔍 Looking for rooted_tip...");
    if let Some(rooted_tip_hash) = source_db.get_cf(&sysconf_cf, "rooted_tip".as_bytes())? {
        println!("  Found rooted_tip hash: {} bytes", rooted_tip_hash.len());

        // Look up the entry by this hash (try "entry" CF first, then "default")
        let default_cf = source_db
            .cf_handle("entry")
            .or_else(|| source_db.cf_handle("default"))
            .ok_or_else(|| anyhow!("entry/default CF not found"))?;

        if let Some(entry_data) = source_db.get_cf(&default_cf, &rooted_tip_hash)? {
            println!("  Found rooted entry: {} bytes", entry_data.len());
            // Parse entry to get height
            match parse_entry_metadata(&entry_data) {
                Ok((height, slot, _hash)) => {
                    rooted_height = height;
                    println!("  Parsed rooted_height: {}, slot: {}", rooted_height, slot);
                }
                Err(e) => {
                    println!("  Failed to parse rooted entry metadata: {}", e);
                    // Try to decode just the top-level structure for debugging
                    if let Ok(term) = Term::decode(&entry_data[..]) {
                        println!("  Entry top-level structure: {:?}", term);
                    }
                }
            }
        } else {
            println!("  rooted_tip hash not found in default CF");
        }
    } else {
        println!("  rooted_tip not found");
    }


    if temporal_height == 0 && rooted_height == 0 {
        return Err(anyhow!("Could not find temporal_height or rooted_height in sysconf CF"));
    }

    // If only one height is found, use a reasonable default for the other
    if temporal_height == 0 {
        temporal_height = rooted_height; // Assume they're the same if temporal not found
    }
    if rooted_height == 0 {
        rooted_height = temporal_height.saturating_sub(10); // Assume rooted is slightly behind
    }

    Ok((temporal_height, rooted_height))
}


fn migrate_contractstate_full(source_db: &DB, target_db: &DB) -> Result<()> {
    println!("🔄 Migrating contractstate (full)...");

    let source_cf = source_db
        .cf_handle("contractstate")
        .ok_or_else(|| anyhow!("contractstate CF not found in source"))?;
    let target_cf = target_db
        .cf_handle("contractstate")
        .ok_or_else(|| anyhow!("contractstate CF not found in target"))?;

    migrate_column_family_full(source_db, &source_cf, target_db, &target_cf, "contractstate")
}

fn migrate_sysconf_full(source_db: &DB, target_db: &DB) -> Result<()> {
    println!("🔄 Migrating sysconf (full)...");

    let source_cf = source_db
        .cf_handle("sysconf")
        .ok_or_else(|| anyhow!("sysconf CF not found in source"))?;
    let target_cf = target_db
        .cf_handle("sysconf")
        .ok_or_else(|| anyhow!("sysconf CF not found in target"))?;

    migrate_column_family_full(source_db, &source_cf, target_db, &target_cf, "sysconf")
}

fn migrate_column_family_full(
    source_db: &DB,
    source_cf: &impl rocksdb::AsColumnFamilyRef,
    target_db: &DB,
    target_cf: &impl rocksdb::AsColumnFamilyRef,
    cf_name: &str,
) -> Result<()> {
    println!("🔄 Migrating {} column family data...", cf_name);

    // First, check if the target database already has data
    let initial_target_count = count_kvs(target_db, target_cf)?;
    if initial_target_count > 0 {
        println!("⚠️  Target database already contains {} kvs. Continue? (y/N)", initial_target_count);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" && input.trim().to_lowercase() != "yes" {
            println!("❌ Migration cancelled by user");
            return Ok(());
        }
        println!("🔄 Continuing with migration (data will be merged)...");
    }
    
    let iter = source_db.iterator_cf(source_cf, rocksdb::IteratorMode::Start);
    let mut count = 0;
    let mut batch_size = 0;
    let mut errors = 0;
    let max_batch_size = 1000; // Process in batches for better performance
    let max_errors = 10; // Stop after 10 consecutive errors
    
    let mut write_batch = rocksdb::WriteBatch::default();
    
    for item in iter {
        match item {
            Ok((key, value)) => {
                // Validate key and value sizes (RocksDB limits)
                if key.len() > 1024 * 1024 { // 1MB key limit
                    println!("⚠️  Skipping key with size {} bytes (too large)", key.len());
                    continue;
                }
                if value.len() > 256 * 1024 * 1024 { // 256MB value limit  
                    println!("⚠️  Skipping value with size {} bytes (too large)", value.len());
                    continue;
                }
                
                // Add to batch
                write_batch.put_cf(target_cf, &key, &value);
                batch_size += 1;
                count += 1;
                errors = 0; // Reset error counter on success
                
                // Write batch when it reaches max size
                if batch_size >= max_batch_size {
                    let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                    match target_db.write(batch_to_write) {
                        Ok(_) => {
                            batch_size = 0;
                            println!("📦 Migrated {} kvs so far...", count);
                        }
                        Err(e) => {
                            println!("❌ Failed to write batch: {}", e);
                            errors += 1;
                            if errors >= max_errors {
                                return Err(anyhow!("Too many consecutive errors during migration"));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("⚠️  Error reading entry: {}", e);
                errors += 1;
                if errors >= max_errors {
                    return Err(anyhow!("Too many consecutive read errors during migration"));
                }
                continue;
            }
        }
    }
    
    // Write remaining entries in the batch
    if batch_size > 0 {
        match target_db.write(write_batch) {
            Ok(_) => println!("📦 Wrote final batch of {} kvs", batch_size),
            Err(e) => return Err(anyhow!("Failed to write final batch: {}", e)),
        }
    }

    // Verify migration by comparing counts
    println!("🔍 Verifying migration...");
    let source_count = count_kvs(source_db, source_cf)?;
    let final_target_count = count_kvs(target_db, target_cf)?;
    let migrated_count = final_target_count - initial_target_count;

    if source_count == migrated_count {
        println!("✅ Migration verification successful: {} kvs migrated", migrated_count);
        println!("📊 Summary:");
        println!("   - Source kvs: {}", source_count);
        println!("   - Target kvs (before): {}", initial_target_count);
        println!("   - Target kvs (after): {}", final_target_count);
        println!("   - Migrated kvs: {}", migrated_count);
    } else {
        return Err(anyhow!(
            "Migration verification failed: source={}, migrated={}, target_total={}",
            source_count,
            migrated_count,
            final_target_count
        ));
    }

    Ok(())
}

fn migrate_default_selective(source_db: &DB, target_db: &DB, temporal_height: u64, rooted_height: u64) -> Result<Vec<Vec<u8>>> {
    println!("🔄 Migrating default CF (selective: temporal to rooted + chain to genesis)...");

    // New Elixir format uses separate "entry" CF, old format uses "default"
    let source_default_cf = source_db
        .cf_handle("entry")
        .or_else(|| source_db.cf_handle("default"))
        .ok_or_else(|| anyhow!("entry/default CF not found in source"))?;
    let target_default_cf = target_db
        .cf_handle("entry")
        .or_else(|| target_db.cf_handle("default"))
        .ok_or_else(|| anyhow!("entry/default CF not found in target"))?;
    let target_entry_by_height_cf = target_db
        .cf_handle("entry_by_height|height->entryhash")
        .or_else(|| target_db.cf_handle("entry_by_height|height:entryhash"))
        .ok_or_else(|| anyhow!("entry_by_height CF not found in target"))?;
    let target_entry_by_slot_cf = target_db
        .cf_handle("entry_by_slot|slot->entryhash")
        .or_else(|| target_db.cf_handle("entry_by_slot|slot:entryhash"))
        .ok_or_else(|| anyhow!("entry_by_slot CF not found in target"))?;

    let mut migrated_entries = 0;
    let mut write_batch = rocksdb::WriteBatch::default();
    let batch_size = 1000;
    let mut temporal_entry_hashes = Vec::new(); // Collect ONLY temporal entry hashes for muts_rev migration
    let mut rooted_entry_hashes = Vec::new(); // Collect rooted entry hashes (for reference, not muts_rev)

    // Phase 1: Migrate entries between temporal_height and rooted_height
    println!("📦 Phase 1: Migrating entries from temporal height {} to rooted height {}", temporal_height, rooted_height);

    // Get source entry_by_height index for efficient lookup (try both naming formats)
    let source_entry_by_height_cf = source_db
        .cf_handle("entry_by_height|height->entryhash")
        .or_else(|| source_db.cf_handle("entry_by_height|height:entryhash"))
        .ok_or_else(|| anyhow!("entry_by_height CF not found in source"))?;

    // Use index-based lookup instead of full table scan
    for height in rooted_height..=temporal_height {
        let height_prefix = format!("{}:", height);
        println!("  Looking for entries at height {} (prefix: '{}')", height, height_prefix);


        let iter = source_db.iterator_cf(&source_entry_by_height_cf, rocksdb::IteratorMode::From(height_prefix.as_bytes(), rocksdb::Direction::Forward));
        for item in iter {
            let (index_key, entry_hash) = item?;

            // Check if we're still in the right height range (binary comparison)
            let height_prefix_bytes = height_prefix.as_bytes();
            if !index_key.starts_with(height_prefix_bytes) {
                break; // Moved past this height
            }

            // Get the actual entry from default CF
            if let Some(entry_data) = source_db.get_cf(&source_default_cf, &entry_hash)? {
                // Parse entry to get slot (we already know height)
                if let Ok((_height, slot, computed_hash)) = parse_entry_metadata(&entry_data) {
                    // Add to default CF
                    write_batch.put_cf(&target_default_cf, &entry_hash, &entry_data);

                    // Add to entry_by_height index (format: "height:hash")
                    let height_key = format!("{}:{}", height, hex::encode(&computed_hash));
                    write_batch.put_cf(&target_entry_by_height_cf, height_key.as_bytes(), &computed_hash);

                    // Add to entry_by_slot index (format: "slot:hash")
                    let slot_key = format!("{}:{}", slot, hex::encode(&computed_hash));
                    write_batch.put_cf(&target_entry_by_slot_cf, slot_key.as_bytes(), &computed_hash);

                    // Collect temporal entry hash for muts_rev migration
                    temporal_entry_hashes.push(entry_hash.to_vec());

                    migrated_entries += 1;

                    if migrated_entries % batch_size == 0 {
                        let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                        target_db.write(batch_to_write)?;
                        println!("📦 Migrated {} entries (Phase 1)...", migrated_entries);
                    }
                }
            }
        }

    }

    // Phase 3: Migrate entries above temporal_height (going upwards until no entries found)
    println!("📦 Phase 3: Migrating entries above temporal height {} (going upwards)", temporal_height);

    let mut current_height = temporal_height + 1;
    let mut above_temporal_entries = 0;
    let mut consecutive_empty_heights = 0;
    let max_consecutive_empty = 5; // Stop after 5 consecutive empty heights

    loop {
        let height_prefix = format!("{}:", current_height);
        let mut found_entries_at_height = false;

        let iter = source_db.iterator_cf(&source_entry_by_height_cf, rocksdb::IteratorMode::From(height_prefix.as_bytes(), rocksdb::Direction::Forward));
        for item in iter {
            let (index_key, entry_hash) = item?;

            // Check if we're still in the right height range
            let height_prefix_bytes = height_prefix.as_bytes();
            if !index_key.starts_with(height_prefix_bytes) {
                break; // Moved past this height
            }

            // Get the actual entry from default CF
            if let Some(entry_data) = source_db.get_cf(&source_default_cf, &entry_hash)? {
                // Parse entry to get slot
                if let Ok((_height, slot, computed_hash)) = parse_entry_metadata(&entry_data) {
                    // Add to default CF
                    write_batch.put_cf(&target_default_cf, &entry_hash, &entry_data);

                    // Add to entry_by_height index
                    let height_key = format!("{}:{}", current_height, hex::encode(&computed_hash));
                    write_batch.put_cf(&target_entry_by_height_cf, height_key.as_bytes(), &computed_hash);

                    // Add to entry_by_slot index
                    let slot_key = format!("{}:{}", slot, hex::encode(&computed_hash));
                    write_batch.put_cf(&target_entry_by_slot_cf, slot_key.as_bytes(), &computed_hash);

                    // Collect entry hash for muts_rev migration (entries above temporal are also temporal-like)
                    temporal_entry_hashes.push(entry_hash.to_vec());

                    above_temporal_entries += 1;
                    found_entries_at_height = true;

                    if above_temporal_entries % batch_size == 0 {
                        let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                        target_db.write(batch_to_write)?;
                        println!("📦 Migrated {} entries above temporal (Phase 3)...", above_temporal_entries);
                    }
                }
            }
        }

        if found_entries_at_height {
            consecutive_empty_heights = 0;
            current_height += 1;
        } else {
            consecutive_empty_heights += 1;
            if consecutive_empty_heights >= max_consecutive_empty {
                println!("📍 No entries found for {} consecutive heights, stopping at height {}", max_consecutive_empty, current_height);
                break;
            }
            current_height += 1;
        }
    }

    println!("✅ Phase 3 complete: {} entries above temporal height migrated", above_temporal_entries);

    // Phase 2: Migrate chain from rooted_height down to genesis (follow prev_hash chain)
    println!("📦 Phase 2: Migrating chain from rooted height {} down to genesis (max 1000 entries)", rooted_height);

    let mut current_height = rooted_height;
    let mut chain_entries = 0;
    let max_chain_entries = 1000;

    loop {
        if chain_entries >= max_chain_entries {
            println!("📍 Reached limit of {} chain entries", max_chain_entries);
            break;
        }
        // Find entry at current_height using index (much faster!)
        if let Some(entry_at_height) = find_entry_at_height_indexed(source_db, &source_entry_by_height_cf, &source_default_cf, current_height)? {
            let (key, value) = entry_at_height;

            // Parse entry to get prev_hash and metadata
            if let Ok((height, slot, entry_hash)) = parse_entry_metadata(&value) {
                // Add to default CF
                write_batch.put_cf(&target_default_cf, &key, &value);

                // Add to indexes (format: "height:hash" and "slot:hash")
                let height_key = format!("{}:{}", height, hex::encode(&entry_hash));
                write_batch.put_cf(&target_entry_by_height_cf, height_key.as_bytes(), &entry_hash);

                let slot_key = format!("{}:{}", slot, hex::encode(&entry_hash));
                write_batch.put_cf(&target_entry_by_slot_cf, slot_key.as_bytes(), &entry_hash);

                // Collect rooted entry hash (for reference, not muts_rev)
                rooted_entry_hashes.push(key.to_vec());

                chain_entries += 1;

                if chain_entries % batch_size == 0 {
                    let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                    target_db.write(batch_to_write)?;
                    println!("📦 Migrated {} chain entries (Phase 2)...", chain_entries);
                }

                // Get prev_hash to continue chain
                if let Some(prev_height) = get_prev_height_from_entry(&value, source_db)? {
                    if prev_height == 0 {
                        println!("📍 Reached genesis at height 0");
                        break;
                    }
                    current_height = prev_height;
                } else {
                    println!("⚠️  Could not get prev_height from entry at height {}, stopping chain", current_height);
                    break;
                }
            } else {
                println!("⚠️  Could not parse entry at height {}, stopping chain", current_height);
                break;
            }
        } else {
            println!("❌ Missing entry at height {}, gap detected", current_height);
            break;
        }

        if current_height == 0 {
            break;
        }
    }

    // Write final batch
    if !write_batch.is_empty() {
        target_db.write(write_batch)?;
    }

    println!("✅ Default CF migration complete:");
    println!("   - Phase 1 (temporal-rooted): {} entries", migrated_entries);
    println!("   - Phase 3 (above temporal): {} entries", above_temporal_entries);
    println!("   - Phase 2 (chain to genesis): {} entries", chain_entries);
    println!("   - Total entries migrated: {}", migrated_entries + above_temporal_entries + chain_entries);
    println!("📊 Collected {} temporal entry hashes for muts_rev migration", temporal_entry_hashes.len());
    println!("📊 Collected {} rooted entry hashes (for reference)", rooted_entry_hashes.len());
    Ok(temporal_entry_hashes)
}

fn migrate_muts_rev_selective(source_db: &DB, target_db: &DB, temporal_entry_hashes: &[Vec<u8>]) -> Result<()> {
    println!("🔄 Migrating muts_rev (for temporal entries only)...");

    let source_cf = source_db
        .cf_handle("muts_rev")
        .ok_or_else(|| anyhow!("muts_rev CF not found in source"))?;
    let target_cf = target_db
        .cf_handle("muts_rev")
        .ok_or_else(|| anyhow!("muts_rev CF not found in target"))?;

    let mut count = 0;
    let mut write_batch = rocksdb::WriteBatch::default();
    let batch_size = 1000;
    let mut not_found = 0;

    // Process only temporal entry hashes (muts_rev is keyed by entry hash)
    for entry_hash in temporal_entry_hashes {
        if let Some(value) = source_db.get_cf(&source_cf, entry_hash)? {
            write_batch.put_cf(&target_cf, entry_hash, &value);
            count += 1;

            if count % batch_size == 0 {
                let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                target_db.write(batch_to_write)?;
                println!("📦 Migrated {} muts_rev kvs...", count);
            }
        } else {
            not_found += 1;
        }
    }

    // Write final batch
    if !write_batch.is_empty() {
        target_db.write(write_batch)?;
    }

    println!("✅ muts_rev migration complete: {} kvs migrated, {} not found in source", count, not_found);
    if not_found > 0 {
        println!("ℹ️  {} temporal entries did not have corresponding muts_rev data (this may be normal)", not_found);
    }
    Ok(())
}

fn write_rooted_height_to_sysconf(target_db: &DB, rooted_height: u64) -> Result<()> {
    println!("🔄 Writing rooted_height to sysconf...");

    let target_cf = target_db
        .cf_handle("sysconf")
        .ok_or_else(|| anyhow!("sysconf CF not found in target"))?;

    // Encode rooted_height as ETF (matching temporal_height format)
    // For blockchain heights, FixInteger (i32) should suffice for reasonable heights
    let mut encoded_height = Vec::new();
    if rooted_height <= i32::MAX as u64 {
        Term::FixInteger(eetf::FixInteger { value: rooted_height as i32 }).encode(&mut encoded_height)?;
    } else {
        // For very large heights, encode as a binary string (will be parsed back as integer)
        let height_str = rooted_height.to_string();
        Term::Binary(eetf::Binary { bytes: height_str.into_bytes() }).encode(&mut encoded_height)?;
    }

    target_db.put_cf(&target_cf, "rooted_height".as_bytes(), &encoded_height)?;
    println!("✅ rooted_height ({}) written to sysconf", rooted_height);
    Ok(())
}

fn migrate_muts_selective(source_db: &DB, target_db: &DB, temporal_entry_hashes: &[Vec<u8>]) -> Result<()> {
    println!("🔄 Migrating muts (for temporal entries only)...");

    let source_cf = source_db
        .cf_handle("muts")
        .ok_or_else(|| anyhow!("muts CF not found in source"))?;
    let target_cf = target_db
        .cf_handle("muts")
        .ok_or_else(|| anyhow!("muts CF not found in target"))?;

    let mut count = 0;
    let mut write_batch = rocksdb::WriteBatch::default();
    let batch_size = 1000;
    let mut not_found = 0;

    for entry_hash in temporal_entry_hashes {
        if let Some(value) = source_db.get_cf(&source_cf, entry_hash)? {
            write_batch.put_cf(&target_cf, entry_hash, &value);
            count += 1;

            if count % batch_size == 0 {
                let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                target_db.write(batch_to_write)?;
                println!("📦 Migrated {} muts kvs...", count);
            }
        } else {
            not_found += 1;
        }
    }

    if !write_batch.is_empty() {
        target_db.write(write_batch)?;
    }

    println!("✅ muts migration complete: {} kvs migrated, {} not found", count, not_found);
    Ok(())
}

fn migrate_my_attestations_selective(source_db: &DB, target_db: &DB, temporal_entry_hashes: &[Vec<u8>]) -> Result<()> {
    println!("🔄 Migrating my_attestation_for_entry (for temporal entries only)...");

    let source_cf = source_db
        .cf_handle("my_attestation_for_entry|entryhash->attestation")
        .or_else(|| source_db.cf_handle("my_attestation_for_entry|entryhash"))
        .ok_or_else(|| anyhow!("my_attestation_for_entry CF not found in source"))?;
    let target_cf = target_db
        .cf_handle("my_attestation_for_entry|entryhash->attestation")
        .or_else(|| target_db.cf_handle("my_attestation_for_entry|entryhash"))
        .ok_or_else(|| anyhow!("my_attestation_for_entry CF not found in target"))?;

    let mut count = 0;
    let mut write_batch = rocksdb::WriteBatch::default();
    let batch_size = 1000;
    let mut not_found = 0;

    for entry_hash in temporal_entry_hashes {
        if let Some(value) = source_db.get_cf(&source_cf, entry_hash)? {
            write_batch.put_cf(&target_cf, entry_hash, &value);
            count += 1;

            if count % batch_size == 0 {
                let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
                target_db.write(batch_to_write)?;
                println!("📦 Migrated {} my_attestations kvs...", count);
            }
        } else {
            not_found += 1;
        }
    }

    if !write_batch.is_empty() {
        target_db.write(write_batch)?;
    }

    println!("✅ my_attestations migration complete: {} kvs migrated, {} not found", count, not_found);
    Ok(())
}

fn migrate_consensus_selective(source_db: &DB, target_db: &DB, temporal_entry_hashes: &[Vec<u8>]) -> Result<()> {
    println!("🔄 Migrating consensus (for temporal entries only)...");

    let source_consensus_cf = source_db
        .cf_handle("consensus")
        .ok_or_else(|| anyhow!("consensus CF not found in source"))?;
    let target_consensus_cf = target_db
        .cf_handle("consensus")
        .ok_or_else(|| anyhow!("consensus CF not found in target"))?;
    let source_consensus_by_entryhash_cf = source_db
        .cf_handle("consensus_by_entryhash|Map<mutationshash,consensus>")
        .ok_or_else(|| anyhow!("consensus_by_entryhash CF not found in source"))?;
    let target_consensus_by_entryhash_cf = target_db
        .cf_handle("consensus_by_entryhash|Map<mutationshash,consensus>")
        .ok_or_else(|| anyhow!("consensus_by_entryhash CF not found in target"))?;

    let mut count_consensus = 0;
    let mut count_by_entryhash = 0;
    let mut write_batch = rocksdb::WriteBatch::default();
    let batch_size = 1000;

    for entry_hash in temporal_entry_hashes {
        // Migrate from consensus CF
        if let Some(value) = source_db.get_cf(&source_consensus_cf, entry_hash)? {
            write_batch.put_cf(&target_consensus_cf, entry_hash, &value);
            count_consensus += 1;
        }

        // Migrate from consensus_by_entryhash CF
        if let Some(value) = source_db.get_cf(&source_consensus_by_entryhash_cf, entry_hash)? {
            write_batch.put_cf(&target_consensus_by_entryhash_cf, entry_hash, &value);
            count_by_entryhash += 1;
        }

        if (count_consensus + count_by_entryhash) % batch_size == 0 {
            let batch_to_write = std::mem::replace(&mut write_batch, rocksdb::WriteBatch::default());
            target_db.write(batch_to_write)?;
            println!("📦 Migrated {} consensus kvs...", count_consensus + count_by_entryhash);
        }
    }

    if !write_batch.is_empty() {
        target_db.write(write_batch)?;
    }

    println!("✅ consensus migration complete: {} consensus, {} by_entryhash migrated", count_consensus, count_by_entryhash);
    Ok(())
}

// Helper functions for entry parsing and chain following

fn parse_entry_metadata(entry_data: &[u8]) -> Result<(u64, u64, Vec<u8>)> {
    // Parse ETF entry to extract height, slot, and compute entry hash
    let term = Term::decode(entry_data)?;

    if let Term::Map(map) = term {
        let mut height = 0u64;
        let mut slot = 0u64;
        let mut header_bin = Vec::new();

        for (key, value) in &map.map {
            if let Term::Atom(atom) = key {
                match atom.name.as_str() {
                    "header" => {
                        if let Term::Binary(binary) = value {
                            header_bin = binary.bytes.clone();

                            // Parse header to get height and slot
                            if let Ok(header_term) = Term::decode(&header_bin[..]) {
                                if let Term::Map(header_map) = header_term {
                                    for (hkey, hvalue) in &header_map.map {
                                        if let Term::Atom(hatom) = hkey {
                                            match hatom.name.as_str() {
                                                "height" => {
                                                    if let Term::BigInteger(big_int) = hvalue {
                                                        height = big_int.value.clone().try_into().unwrap_or(0);
                                                    } else if let Term::FixInteger(fix_int) = hvalue {
                                                        height = fix_int.value as u64;
                                                    }
                                                }
                                                "slot" => {
                                                    if let Term::BigInteger(big_int) = hvalue {
                                                        slot = big_int.value.clone().try_into().unwrap_or(0);
                                                    } else if let Term::FixInteger(fix_int) = hvalue {
                                                        slot = fix_int.value as u64;
                                                    }
                                                }
                                                _ => continue,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }

        // Compute entry hash (blake3 of header_bin)
        let entry_hash = blake3::hash(&header_bin);

        Ok((height, slot, entry_hash.as_bytes().to_vec()))
    } else {
        Err(anyhow!("Entry is not an ETF map"))
    }
}

fn find_entry_at_height_indexed(
    source_db: &DB,
    source_entry_by_height_cf: &impl rocksdb::AsColumnFamilyRef,
    source_default_cf: &impl rocksdb::AsColumnFamilyRef,
    height: u64
) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    // Use the entry_by_height index for O(log n) lookup instead of O(n) scan
    let height_prefix = format!("{}:", height);

    let iter = source_db.iterator_cf(source_entry_by_height_cf, rocksdb::IteratorMode::From(height_prefix.as_bytes(), rocksdb::Direction::Forward));
    for item in iter {
        let (index_key, entry_hash) = item?;

        // Check if we're still in the right height range (binary comparison)
        let height_prefix_bytes = height_prefix.as_bytes();
        if !index_key.starts_with(height_prefix_bytes) {
            break; // Moved past this height, no entries found
        }

        // Get the first entry at this height (there might be multiple, but we just need one for the chain)
        if let Some(entry_data) = source_db.get_cf(source_default_cf, &entry_hash)? {
            return Ok(Some((entry_hash.to_vec(), entry_data)));
        }
    }

    Ok(None)
}

fn get_prev_height_from_entry(entry_data: &[u8], source_db: &DB) -> Result<Option<u64>> {
    // Parse entry to get header, then extract prev_hash, then lookup prev entry height
    let term = Term::decode(entry_data)?;
    if let Term::Map(map) = term {
        // Get header binary from entry
        if let Some(header_data) = map.map.get(&Term::Atom(eetf::Atom::from("header"))) {
            if let Term::Binary(binary) = header_data {
                // Parse header to get prev_hash
                let header_term = Term::decode(&binary.bytes[..])?;
                if let Term::Map(header_map) = header_term {
                    if let Some(prev_hash_term) = header_map.map.get(&Term::Atom(eetf::Atom::from("prev_hash"))) {
                        if let Term::Binary(prev_hash_bin) = prev_hash_term {
                            if prev_hash_bin.bytes.len() == 32 {
                                let prev_hash: [u8; 32] = prev_hash_bin.bytes.as_slice().try_into().unwrap();

                                // Special case: all-zero hash means genesis
                                if prev_hash == [0u8; 32] {
                                    return Ok(Some(0));
                                }

                                // Look up previous entry by hash to get its height (try "entry" first, then "default")
                                let default_cf = source_db.cf_handle("entry")
                                    .or_else(|| source_db.cf_handle("default"))
                                    .ok_or_else(|| anyhow!("entry/default CF not found"))?;

                                if let Some(prev_entry_data) = source_db.get_cf(&default_cf, &prev_hash)? {
                                    if let Ok((prev_height, _slot, _hash)) = parse_entry_metadata(&prev_entry_data) {
                                        return Ok(Some(prev_height));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}


fn count_kvs(db: &DB, cf: &impl rocksdb::AsColumnFamilyRef) -> Result<usize> {
    let iter = db.iterator_cf(cf, rocksdb::IteratorMode::Start);
    let mut count = 0;
    for item in iter {
        let _ = item?;
        count += 1;
    }
    Ok(count)
}

fn show_tips(db: &DB) -> Result<()> {
    println!("📊 Blockchain Tips from sysconf");
    println!("{}", "=".repeat(80));

    let sysconf_cf = db
        .cf_handle("sysconf")
        .ok_or_else(|| anyhow!("sysconf column family not found"))?;

    // Get temporal_height
    if let Some(value) = db.get_cf(&sysconf_cf, "temporal_height".as_bytes())? {
        if let Ok(term) = Term::decode(&value[..]) {
            let height = match term {
                Term::BigInteger(big_int) => big_int.value.clone().try_into().unwrap_or(0),
                Term::FixInteger(fix_int) => fix_int.value as u64,
                _ => 0,
            };
            println!("Temporal Height: {}", height);
        }
    } else {
        println!("Temporal Height: (not found)");
    }

    // Check for temporal_tip (might not exist)
    if let Some(value) = db.get_cf(&sysconf_cf, "temporal_tip".as_bytes())? {
        println!("Temporal Tip:    {}", hex::encode(&value));
    }

    // Get rooted_tip
    if let Some(rooted_tip_hash) = db.get_cf(&sysconf_cf, "rooted_tip".as_bytes())? {
        println!("Rooted Tip:      {}", hex::encode(&rooted_tip_hash));

        // Try to get the height of the rooted tip
        let default_cf = db
            .cf_handle("default")
            .ok_or_else(|| anyhow!("default CF not found"))?;

        if let Some(entry_data) = db.get_cf(&default_cf, &rooted_tip_hash)? {
            if let Ok((height, slot, _hash)) = parse_entry_metadata(&entry_data) {
                println!("Rooted Height:   {}", height);
                println!("Rooted Slot:     {}", slot);
            }
        }
    } else {
        println!("Rooted Tip:      (not found)");
    }

    println!("{}", "=".repeat(80));
    Ok(())
}

fn list_keys(db: &DB, cf_handle: &impl rocksdb::AsColumnFamilyRef) -> Result<()> {
    let iter = db.iterator_cf(cf_handle, rocksdb::IteratorMode::Start);
    let mut count = 0;

    println!("Keys in contractstate column family:");
    println!("{:<4} {:<50} {}", "No.", "Decoded Key", "Raw Hex");
    println!("{:-<80}", "");
    
    for item in iter {
        let (key, _) = item?;
        let decoded_key = decode_contractstate_key(&key);
        let hex_key = hex::encode(&key);
        
        // Truncate very long keys for display
        let display_decoded = if decoded_key.len() > 45 {
            format!("{}...", &decoded_key[..42])
        } else {
            decoded_key
        };
        
        let display_hex = if hex_key.len() > 20 {
            format!("{}...", &hex_key[..17])
        } else {
            hex_key
        };
        
        println!("{:<4} {:<50} {}", count, display_decoded, display_hex);
        count += 1;

        if count >= 100 {
            println!("... (showing first 100 keys, use --export to see all)");
            break;
        }
    }

    println!("\nTotal keys shown: {}", count);
    Ok(())
}

fn get_value(
    db: &DB,
    cf_handle: &impl rocksdb::AsColumnFamilyRef,
    key_hex: &str,
    raw: bool,
) -> Result<()> {
    let key = hex::decode(key_hex)?;
    
    if let Some(value) = db.get_cf(cf_handle, &key)? {
        println!("Key: {}", key_hex);
        println!("Raw value size: {} bytes", value.len());
        
        if raw {
            println!("Raw hex: {}", hex::encode(&value));
        } else {
            match parse_etf_to_json(&value) {
                Ok(json) => {
                    println!("Parsed value: {}", serde_json::to_string_pretty(&json)?);
                },
                Err(e) => {
                    println!("Failed to parse: {}", e);
                    println!("Raw hex: {}", hex::encode(&value));
                }
            }
        }
    } else {
        println!("Key not found: {}", key_hex);
    }

    Ok(())
}

fn export_all_data(
    db: &DB,
    cf_handle: &impl rocksdb::AsColumnFamilyRef,
    output_file: &str,
    raw: bool,
) -> Result<()> {
    let iter = db.iterator_cf(cf_handle, rocksdb::IteratorMode::Start);
    let mut data = serde_json::Map::new();
    let mut count = 0;
    let mut failed_parse_count = 0;

    println!("Exporting contractstate data to {}...", output_file);

    for item in iter {
        let (key, value) = item?;
        
        // Use decoded key as the JSON key, with hex fallback
        let json_key = decode_contractstate_key(&key);

        if raw {
            data.insert(json_key, json!(hex::encode(&value)));
        } else {
            match parse_etf_to_json(&value) {
                Ok(parsed_value) => {
                    data.insert(json_key, parsed_value);
                }
                Err(e) => {
                    failed_parse_count += 1;
                    // Store with error details
                    data.insert(
                        json_key.clone(),
                        json!({
                            "parse_error": format!("Failed to parse: {}", e),
                            "raw_hex": hex::encode(&value),
                            "as_string": std::str::from_utf8(&value).unwrap_or("<invalid UTF-8>"),
                            "size_bytes": value.len()
                        }),
                    );
                }
            }
        }

        count += 1;
        if count % 1000 == 0 {
            println!("Processed {} entries...", count);
        }
    }

    let final_json = json!({
        "metadata": {
            "total_entries": count,
            "failed_parses": failed_parse_count,
            "export_time": chrono::Utc::now().to_rfc3339(),
            "raw_mode": raw,
            "key_decoding": "enabled"
        },
        "data": data
    });

    std::fs::write(output_file, serde_json::to_string_pretty(&final_json)?)?;
    println!(
        "Export complete: {} entries, {} failed parses",
        count, failed_parse_count
    );

    Ok(())
}

fn decode_contractstate_key(key: &[u8]) -> String {
    // Try to decode the key as a meaningful string with Base58 public keys
    if let Ok(key_str) = std::str::from_utf8(key) {
        // If it's already a valid UTF-8 string, return it as-is
        return key_str.to_string();
    }
    
    // Handle mixed binary/text keys
    // These typically start with a text prefix, followed by binary data, possibly more text
    let mut result = String::new();
    let mut pos = 0;
    
    // Common prefixes in contractstate
    let prefixes = [
        "bic:coin:balance:",
        "bic:epoch:trainers:",
        "bic:epoch:pop:",
        "bic:base:nonce:",
        "bic:epoch:emission_address:",
        "bic:epoch:segment_vr_hash",
        "bic:epoch:solutions_count:",
        "bic:contract:account:",
        "bic:coin:",
        "bic:epoch:",
    ];
    
    // Try to find a matching prefix
    for prefix in &prefixes {
        if key.starts_with(prefix.as_bytes()) {
            result.push_str(prefix);
            pos = prefix.len();
            break;
        }
    }
    
    if pos == 0 {
        // No recognized prefix, try parsing as string or return hex
        if let Ok(s) = std::str::from_utf8(key) {
            return s.to_string();
        } else {
            return format!("hex:{}", hex::encode(key));
        }
    }
    
    // Parse the rest of the key
    while pos < key.len() {
        // Try to detect patterns in the remaining data
        let remaining = &key[pos..];
        
        // Check for 48-byte public key (Base58 encoded in display)
        if remaining.len() >= 48 {
            let maybe_pk = &remaining[0..48];
            // Check if this looks like a public key (not all zeros or all 0xFF)
            if !maybe_pk.iter().all(|&b| b == 0) && !maybe_pk.iter().all(|&b| b == 0xFF) {
                // This looks like a public key, encode it in Base58
                let base58_pk = bs58::encode(maybe_pk).into_string();
                result.push_str(&base58_pk);
                pos += 48;
                
                // Check if there's more data after the public key
                if pos < key.len() {
                    let remainder = &key[pos..];
                    // Try to parse remainder as string (like ":AMA" suffix)
                    if let Ok(suffix) = std::str::from_utf8(remainder) {
                        result.push_str(suffix);
                        break;
                    }
                }
                continue;
            }
        }
        
        // Check for 12-digit height padding (like "000000319557")
        if remaining.len() >= 12 {
            let maybe_height = &remaining[0..12];
            if maybe_height.iter().all(|&b| b.is_ascii_digit()) {
                if let Ok(height_str) = std::str::from_utf8(maybe_height) {
                    result.push_str(height_str);
                    pos += 12;
                    continue;
                }
            }
        }
        
        // Check for 20-digit nonce padding
        if remaining.len() >= 20 {
            let maybe_nonce = &remaining[0..20];
            if maybe_nonce.iter().all(|&b| b.is_ascii_digit()) {
                if let Ok(nonce_str) = std::str::from_utf8(maybe_nonce) {
                    result.push_str(nonce_str);
                    pos += 20;
                    continue;
                }
            }
        }
        
        // Try to parse remaining bytes as a string
        if let Ok(remainder_str) = std::str::from_utf8(remaining) {
            result.push_str(remainder_str);
            break;
        }
        
        // If we can't parse the rest, append as hex and break
        result.push_str(&format!(":hex:{}", hex::encode(remaining)));
        break;
    }
    
    result
}

fn parse_etf_to_json(data: &[u8]) -> Result<serde_json::Value> {
    // First, try to parse as a string (many values in contractstate are plain strings)
    if let Ok(string_val) = std::str::from_utf8(data) {
        // If it's a valid UTF-8 string and doesn't start with ETF magic byte (131)
        if !data.starts_with(&[131]) {
            // Try parsing as integer first
            if let Ok(int_val) = string_val.parse::<i64>() {
                return Ok(json!(int_val));
            }
            // Otherwise return as string
            return Ok(json!(string_val));
        }
    }

    // Check if it starts with ETF magic byte (131)
    if data.starts_with(&[131]) {
        // Try to parse with eetf crate
        match Term::decode(data) {
            Ok(term) => {
                // Convert ETF term to JSON
                return etf_term_to_json(&term);
            }
            Err(e) => {
                // If parsing fails, return error info with basic ETF structure
                let mut etf_info = serde_json::Map::new();
                etf_info.insert("etf_format".to_string(), json!(true));
                etf_info.insert("parse_error".to_string(), json!(format!("{}", e)));
                etf_info.insert("magic_byte".to_string(), json!(131));
                etf_info.insert("data_size".to_string(), json!(data.len()));
                etf_info.insert("raw_hex".to_string(), json!(hex::encode(data)));

                // Try to give some indication of the ETF type
                if data.len() > 1 {
                    let type_byte = data[1];
                    etf_info.insert("etf_type_byte".to_string(), json!(type_byte));
                    let type_name = match type_byte {
                        70 => "NEW_FLOAT",
                        97 => "SMALL_INTEGER",
                        98 => "INTEGER",
                        100 => "ATOM",
                        104 => "SMALL_TUPLE",
                        105 => "LARGE_TUPLE",
                        106 => "NIL",
                        107 => "STRING",
                        108 => "LIST",
                        109 => "BINARY",
                        116 => "MAP",
                        119 => "SMALL_ATOM",
                        _ => "UNKNOWN"
                    };
                    etf_info.insert("etf_type".to_string(), json!(type_name));
                }

                return Ok(json!(etf_info));
            }
        }
    }

    // If not ETF and not valid UTF-8, return as hex
    Ok(json!({
        "raw_hex": hex::encode(data),
        "size_bytes": data.len(),
        "note": "Binary data, not ETF format"
    }))
}

fn etf_term_to_json(term: &Term) -> Result<serde_json::Value> {
    match term {
        Term::Atom(atom) => Ok(json!(atom.name)),
        Term::FixInteger(int) => Ok(json!(int.value)),
        Term::BigInteger(big_int) => Ok(json!(big_int.value.to_string())),
        Term::Float(float) => Ok(json!(float.value)),
        Term::Binary(binary) => {
            // Try to decode as UTF-8 string first
            if let Ok(s) = std::str::from_utf8(&binary.bytes) {
                Ok(json!(s))
            } else {
                Ok(json!(hex::encode(&binary.bytes)))
            }
        }
        Term::List(list) => {
            let mut json_list = Vec::new();
            for element in &list.elements {
                json_list.push(etf_term_to_json(element)?);
            }
            Ok(json!(json_list))
        }
        Term::Tuple(tuple) => {
            let mut json_tuple = Vec::new();
            for element in &tuple.elements {
                json_tuple.push(etf_term_to_json(element)?);
            }
            Ok(json!(json_tuple))
        }
        Term::Map(map) => {
            let mut json_map = serde_json::Map::new();
            for (key, value) in &map.map {
                let key_str = match key {
                    Term::Atom(atom) => atom.name.clone(),
                    Term::Binary(binary) => {
                        if let Ok(s) = std::str::from_utf8(&binary.bytes) {
                            s.to_string()
                        } else {
                            format!("binary:{}", hex::encode(&binary.bytes))
                        }
                    }
                    _ => format!("{:?}", key),
                };
                json_map.insert(key_str, etf_term_to_json(value)?);
            }
            Ok(json!(json_map))
        }
        Term::ByteList(byte_list) => {
            if let Ok(s) = std::str::from_utf8(&byte_list.bytes) {
                Ok(json!(s))
            } else {
                Ok(json!(hex::encode(&byte_list.bytes)))
            }
        }
        _ => {
            // For other types (Pid, Port, Reference, etc.), return debug representation
            Ok(json!(format!("{:?}", term)))
        }
    }
}

fn test_entry_hash_verification(db: &DB, output_file: &str) -> Result<()> {
    println!("🧪 Testing entry hash verification...");

    // Get the default column family which contains the entries
    let default_cf = db
        .cf_handle("default")
        .ok_or_else(|| anyhow!("default column family not found"))?;

    // Extract a few entries for testing
    let iter = db.iterator_cf(&default_cf, rocksdb::IteratorMode::Start);
    let mut entries_tested = 0;
    let mut matches = 0;
    let max_entries_to_test = 5;
    let mut test_results = Vec::new();

    println!("📖 Extracting up to {} entries from default CF for verification...", max_entries_to_test);

    for item in iter {
        if entries_tested >= max_entries_to_test {
            break;
        }

        let (key, value) = item?;

        match test_single_entry_hash(&key, &value) {
            Ok((hash_matches, stored_hash, computed_hash)) => {
                entries_tested += 1;
                if hash_matches {
                    matches += 1;
                    println!("✅ Entry {}: Hash verification PASSED", entries_tested);
                } else {
                    println!("❌ Entry {}: Hash verification FAILED", entries_tested);
                }

                // Store test result
                test_results.push(json!({
                    "entry_number": entries_tested,
                    "entry_key": hex::encode(&key),
                    "stored_hash": hex::encode(&stored_hash),
                    "computed_hash": hex::encode(&computed_hash),
                    "hash_matches": hash_matches,
                    "entry_size_bytes": value.len()
                }));
            }
            Err(e) => {
                println!("⚠️  Entry {}: Could not verify hash - {}", entries_tested + 1, e);

                // Store error result
                test_results.push(json!({
                    "entry_number": entries_tested + 1,
                    "entry_key": hex::encode(&key),
                    "error": format!("{}", e),
                    "entry_size_bytes": value.len()
                }));
            }
        }
    }

    println!("\n📊 Test Results:");
    println!("   Entries tested: {}", entries_tested);
    println!("   Hash matches: {}", matches);
    println!("   Success rate: {:.1}%", if entries_tested > 0 { (matches as f64 / entries_tested as f64) * 100.0 } else { 0.0 });

    // Create the output JSON
    let output_json = json!({
        "metadata": {
            "test_time": chrono::Utc::now().to_rfc3339(),
            "entries_tested": entries_tested,
            "hash_matches": matches,
            "success_rate_percent": if entries_tested > 0 { (matches as f64 / entries_tested as f64) * 100.0 } else { 0.0 },
            "max_entries_tested": max_entries_to_test
        },
        "test_results": test_results
    });

    // Save to file
    std::fs::write(output_file, serde_json::to_string_pretty(&output_json)?)?;
    println!("💾 Test results saved to: {}", output_file);

    if matches == entries_tested && entries_tested > 0 {
        println!("🎉 All tested entries have valid hashes!");
    } else if entries_tested == 0 {
        println!("⚠️  No entries found to test");
    } else {
        println!("⚠️  Some entries failed hash verification");
    }

    Ok(())
}

fn test_single_entry_hash(entry_key: &[u8], entry_packed: &[u8]) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    // Parse the ETF-encoded entry
    if !entry_packed.starts_with(&[131]) {
        return Err(anyhow!("Entry does not start with ETF magic byte (131)"));
    }

    // For now, we'll implement a basic check using the approach from the Elixir code
    // The entry structure should be an ETF map with header, txs, hash, signature fields

    // Try to extract the stored hash from the entry
    let stored_hash = extract_hash_from_entry(entry_packed)?;

    // Compute the hash using the same algorithm as the node
    let computed_hash = compute_entry_hash(entry_packed)?;

    let hash_matches = stored_hash == computed_hash;

    if !hash_matches {
        println!("   Key: {}", hex::encode(entry_key));
        println!("   Stored hash:  {}", hex::encode(&stored_hash));
        println!("   Computed hash: {}", hex::encode(&computed_hash));
    }

    Ok((hash_matches, stored_hash, computed_hash))
}

fn extract_hash_from_entry(entry_packed: &[u8]) -> Result<Vec<u8>> {
    // Parse the ETF entry and extract the hash field
    let term = Term::decode(entry_packed)?;

    if let Term::Map(map) = term {
        // Look for the "hash" field in the entry map
        for (key, value) in &map.map {
            if let Term::Atom(atom) = key {
                if atom.name == "hash" {
                    if let Term::Binary(binary) = value {
                        return Ok(binary.bytes.clone());
                    }
                }
            }
        }
        return Err(anyhow!("No 'hash' field found in entry map"));
    }

    Err(anyhow!("Entry is not an ETF map"))
}

fn compute_entry_hash(entry_packed: &[u8]) -> Result<Vec<u8>> {
    // Parse the ETF entry and extract the header binary
    let header_bin = extract_header_binary_from_entry(entry_packed)?;

    // Compute Blake3 hash of the header binary
    // This matches the rs_node implementation: blake3::hash(&self.header_bin)
    let hash = blake3::hash(&header_bin);
    Ok(hash.as_bytes().to_vec())
}

fn extract_header_binary_from_entry(entry_packed: &[u8]) -> Result<Vec<u8>> {
    // Parse the ETF entry and extract the header binary field
    let term = Term::decode(entry_packed)?;

    if let Term::Map(map) = term {
        // Look for the "header" field in the entry map
        for (key, value) in &map.map {
            if let Term::Atom(atom) = key {
                if atom.name == "header" {
                    if let Term::Binary(binary) = value {
                        return Ok(binary.bytes.clone());
                    }
                }
            }
        }
        return Err(anyhow!("No 'header' field found in entry map"));
    }

    Err(anyhow!("Entry is not an ETF map"))
}

