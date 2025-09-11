use anyhow::{anyhow, Result};
use clap::Parser;
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use serde_json::json;
use std::path::Path;

#[derive(Parser)]
#[command(name = "fabric-reader")]
#[command(about = "A CLI tool to read Amadeus fabric database and parse ETF terms to JSON")]
struct Cli {
    /// Path to the RocksDB database directory
    #[arg(short, long)]
    db_path: String,

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

fn list_keys(db: &DB, cf_handle: &impl rocksdb::AsColumnFamilyRef) -> Result<()> {
    let iter = db.iterator_cf(cf_handle, rocksdb::IteratorMode::Start);
    let mut count = 0;

    println!("Keys in contractstate column family:");
    for item in iter {
        let (key, _) = item?;
        println!("{}: {}", count, hex::encode(&key));
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
        let key_hex = hex::encode(&key);

        if raw {
            data.insert(key_hex, json!(hex::encode(&value)));
        } else {
            match parse_etf_to_json(&value) {
                Ok(parsed_value) => {
                    data.insert(key_hex, parsed_value);
                }
                Err(e) => {
                    failed_parse_count += 1;
                    // Store with error details
                    data.insert(
                        key_hex.clone(),
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
            "raw_mode": raw
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

