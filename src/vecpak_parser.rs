use crate::vecpak::Term;
use anyhow::{Result, anyhow};
use rocksdb::DB;

pub fn get_prev_height_from_vecpak_entry(entry_term: &Term, source_db: &DB) -> Result<Option<u64>> {
    // Entry is a PropList/Map with keys: header, txs, hash, signature, mask, etc.
    if let Term::PropList(props) = entry_term {
        println!("  [vecpak] Entry is PropList with {} properties", props.len());
        // Find "header" key
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                let key_str = String::from_utf8_lossy(key_bytes);
                if key_bytes == b"header" {
                    println!("  [vecpak] Found header key");
                    // Header is itself a binary that contains another encoded term
                    if let Term::Binary(header_bytes) = value {
                        println!("  [vecpak] Header is binary, {} bytes", header_bytes.len());
                        // Decode header as vecpak term
                        match crate::vecpak::decode_term_from_slice(header_bytes) {
                            Ok(header_term) => {
                                println!("  [vecpak] Header decoded successfully");
                                return get_prev_hash_from_header(&header_term, source_db);
                            }
                            Err(e) => {
                                println!("  [vecpak] Failed to decode header: {}", e);
                            }
                        }
                    } else {
                        println!("  [vecpak] Header is not a binary: {:?}", value);
                    }
                } else if key_bytes.len() < 20 {
                    println!("  [vecpak] Found key: {}", key_str);
                }
            }
        }
    } else {
        println!("  [vecpak] Entry is not a PropList: {:?}", entry_term);
    }

    println!("  [vecpak] Could not find header, returning None");
    Ok(None)
}

fn get_prev_hash_from_header(header_term: &Term, source_db: &DB) -> Result<Option<u64>> {
    // Header is a PropList with keys like: prev_hash, height, slot, etc.
    if let Term::PropList(props) = header_term {
        println!("  [vecpak] Header is PropList with {} properties", props.len());
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                let key_str = String::from_utf8_lossy(key_bytes);
                if key_bytes == b"prev_hash" {
                    println!("  [vecpak] Found prev_hash key");
                    if let Term::Binary(prev_hash_bytes) = value {
                        if prev_hash_bytes.len() == 32 {
                            let prev_hash: [u8; 32] = prev_hash_bytes[..].try_into().unwrap();

                            // All-zero hash means genesis
                            if prev_hash == [0u8; 32] {
                                return Ok(Some(0));
                            }

                            // Look up previous entry by hash to get its height
                            let entry_cf = source_db.cf_handle("entry")
                                .ok_or_else(|| anyhow!("entry CF not found"))?;

                            if let Some(prev_entry_data) = source_db.get_cf(&entry_cf, &prev_hash)? {
                                // Decode previous entry to get its height
                                if let Ok(prev_entry_term) = crate::vecpak::decode_term_from_slice(&prev_entry_data) {
                                    return get_height_from_entry(&prev_entry_term);
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

fn get_height_from_entry(entry_term: &Term) -> Result<Option<u64>> {
    // Entry is a PropList with header key
    if let Term::PropList(props) = entry_term {
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                if key_bytes == b"header" {
                    if let Term::Binary(header_bytes) = value {
                        if let Ok(header_term) = crate::vecpak::decode_term_from_slice(header_bytes) {
                            return get_height_from_header(&header_term);
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

fn get_height_from_header(header_term: &Term) -> Result<Option<u64>> {
    if let Term::PropList(props) = header_term {
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                if key_bytes == b"height" {
                    if let Term::VarInt(height) = value {
                        if *height >= 0 {
                            return Ok(Some(*height as u64));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}
