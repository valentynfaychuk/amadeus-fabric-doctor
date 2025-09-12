use anyhow::{anyhow, Result};
use clap::Parser;
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde_json::json;
use std::path::Path;

#[derive(Parser)]
#[command(name = "fabric-reader")]
#[command(about = "A CLI tool to read Amadeus fabric database and parse ETF terms to JSON with migration support")]
struct Cli {
    /// Path to the RocksDB database directory (source for migration)
    #[arg(short, long)]
    db_path: String,

    /// Path to the second RocksDB database directory (target for migration)
    #[arg(long)]
    db2_path: Option<String>,

    /// Migrate contractstate column family from db-path to db2-path
    #[arg(long, requires = "db2_path")]
    migrate: bool,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.migrate {
        if let Some(db2_path) = cli.db2_path {
            perform_migration(&cli.db_path, &db2_path)?;
        } else {
            return Err(anyhow!("--db2-path is required for migration"));
        }
    } else {
        // Open the database with contractstate column family
        let db = open_fabric_database(&cli.db_path)?;

        let contractstate_cf = db
            .cf_handle("contractstate")
            .ok_or_else(|| anyhow!("contractstate column family not found"))?;

        if cli.list_keys {
            list_keys(&db, &contractstate_cf)?;
        } else if let Some(key_hex) = cli.key {
            get_value(&db, &contractstate_cf, &key_hex, cli.raw)?;
        } else if let Some(output_file) = cli.export {
            export_all_data(&db, &contractstate_cf, &output_file, cli.raw)?;
        } else {
            println!("Use --help to see available options");
        }
    }

    Ok(())
}

fn open_fabric_database(db_path: &str) -> Result<DB> {
    if !Path::new(db_path).exists() {
        return Err(anyhow!("Database path does not exist: {}", db_path));
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

    let db = DB::open_cf_descriptors(&opts, db_path, cf_descriptors)?;
    Ok(db)
}

fn perform_migration(source_db_path: &str, target_db_path: &str) -> Result<()> {
    println!("🔄 Starting migration from {} to {}", source_db_path, target_db_path);
    
    // Validate source database exists
    if !Path::new(source_db_path).exists() {
        return Err(anyhow!("Source database path does not exist: {}", source_db_path));
    }
    
    // Create target database if it doesn't exist
    create_target_database(target_db_path)?;
    
    // Open source database (old RocksDB version compatible)
    println!("📖 Opening source database...");
    let source_db = open_fabric_database(source_db_path)?;
    let source_contractstate_cf = source_db
        .cf_handle("contractstate")
        .ok_or_else(|| anyhow!("contractstate column family not found in source database"))?;
    
    // Open target database (new RocksDB version)
    println!("🎯 Opening target database...");
    let target_db = open_fabric_database(target_db_path)?;
    let target_contractstate_cf = target_db
        .cf_handle("contractstate")
        .ok_or_else(|| anyhow!("contractstate column family not found in target database"))?;
    
    // Perform the migration
    migrate_contractstate(&source_db, &source_contractstate_cf, &target_db, &target_contractstate_cf)?;
    
    println!("✅ Migration completed successfully!");
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
    
    // Define all the column families that exist in the Amadeus fabric database
    let cf_names = vec![
        "default",
        "entry_by_height|height:entryhash",
        "entry_by_slot|slot:entryhash",
        "tx|txhash:entryhash",
        "tx_account_nonce|account:nonce->txhash",
        "tx_receiver_nonce|receiver:nonce->txhash",
        "my_seen_time_entry|entryhash",
        "my_attestation_for_entry|entryhash",
        "consensus",
        "consensus_by_entryhash|Map<mutationshash,consensus>",
        "contractstate", // This is the one we're migrating
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

fn migrate_contractstate(
    source_db: &DB,
    source_cf: &impl rocksdb::AsColumnFamilyRef,
    target_db: &DB,
    target_cf: &impl rocksdb::AsColumnFamilyRef,
) -> Result<()> {
    println!("🔄 Migrating contractstate column family data...");
    
    // First, check if the target database already has data
    let initial_target_count = count_entries(target_db, target_cf)?;
    if initial_target_count > 0 {
        println!("⚠️  Target database already contains {} entries. Continue? (y/N)", initial_target_count);
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
                            println!("📦 Migrated {} entries so far...", count);
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
            Ok(_) => println!("📦 Wrote final batch of {} entries", batch_size),
            Err(e) => return Err(anyhow!("Failed to write final batch: {}", e)),
        }
    }
    
    // Verify migration by comparing counts
    println!("🔍 Verifying migration...");
    let source_count = count_entries(source_db, source_cf)?;
    let final_target_count = count_entries(target_db, target_cf)?;
    let migrated_count = final_target_count - initial_target_count;
    
    if source_count == migrated_count {
        println!("✅ Migration verification successful: {} entries migrated", migrated_count);
        println!("📊 Summary:");
        println!("   - Source entries: {}", source_count);
        println!("   - Target entries (before): {}", initial_target_count);
        println!("   - Target entries (after): {}", final_target_count);
        println!("   - Migrated entries: {}", migrated_count);
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

fn count_entries(db: &DB, cf: &impl rocksdb::AsColumnFamilyRef) -> Result<usize> {
    let iter = db.iterator_cf(cf, rocksdb::IteratorMode::Start);
    let mut count = 0;
    for item in iter {
        let _ = item?;
        count += 1;
    }
    Ok(count)
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
        // For now, we'll provide basic ETF structure info rather than full parsing
        // This is because ETF parsing requires complex handling and the eetf crate
        // had API compatibility issues
        let mut etf_info = serde_json::Map::new();
        etf_info.insert("etf_format".to_string(), json!(true));
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
                _ => "UNKNOWN"
            };
            etf_info.insert("etf_type".to_string(), json!(type_name));
        }
        
        return Ok(json!(etf_info));
    }
    
    // If not ETF and not valid UTF-8, return as hex
    Ok(json!({
        "raw_hex": hex::encode(data),
        "size_bytes": data.len(),
        "note": "Binary data, not ETF format"
    }))
}
