use crate::vecpak::Term;
use anyhow::{Result, anyhow};
use rocksdb::DB;

pub fn get_prev_height_from_vecpak_entry(entry_term: &Term, source_db: &DB) -> Result<Option<u64>> {
    if let Term::PropList(props) = entry_term {
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                if key_bytes == b"header" {
                    // Header can be Binary (encoded) or PropList (already decoded)
                    match value {
                        Term::Binary(header_bytes) => {
                            if let Ok(header_term) = crate::vecpak::decode_term_from_slice(header_bytes) {
                                return get_prev_hash_from_header(&header_term, source_db);
                            }
                        }
                        Term::PropList(_) => {
                            return get_prev_hash_from_header(value, source_db);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(None)
}

fn get_prev_hash_from_header(header_term: &Term, source_db: &DB) -> Result<Option<u64>> {
    if let Term::PropList(props) = header_term {
        println!("  [DEBUG] Searching for prev_hash in header with {} props", props.len());
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                let key_str = String::from_utf8_lossy(key_bytes);
                println!("  [DEBUG] Checking key: {}", key_str);
                if key_bytes == b"prev_hash" {
                    println!("  [DEBUG] FOUND prev_hash!");
                    if let Term::Binary(prev_hash_bytes) = value {
                        println!("  [DEBUG] prev_hash is Binary, {} bytes", prev_hash_bytes.len());
                        if prev_hash_bytes.len() == 32 {
                            let prev_hash: [u8; 32] = prev_hash_bytes[..].try_into().unwrap();

                            // All-zero hash means genesis
                            if prev_hash == [0u8; 32] {
                                println!("  [DEBUG] prev_hash is all zeros (genesis)");
                                return Ok(Some(0));
                            }

                            println!("  [DEBUG] Looking up prev entry by hash: {}", hex::encode(&prev_hash));
                            // Look up previous entry by hash to get its height
                            let entry_cf = source_db.cf_handle("entry")
                                .ok_or_else(|| anyhow!("entry CF not found"))?;

                            if let Some(prev_entry_data) = source_db.get_cf(&entry_cf, &prev_hash)? {
                                println!("  [DEBUG] Found prev entry, {} bytes", prev_entry_data.len());
                                // Decode previous entry to get its height
                                if let Ok(prev_entry_term) = crate::vecpak::decode_term_from_slice(&prev_entry_data) {
                                    println!("  [DEBUG] Decoded prev entry, getting height");
                                    return get_height_from_entry(&prev_entry_term);
                                } else {
                                    println!("  [DEBUG] Failed to decode prev entry");
                                }
                            } else {
                                println!("  [DEBUG] prev entry not found in entry CF");
                            }
                        } else {
                            println!("  [DEBUG] prev_hash wrong length: {}", prev_hash_bytes.len());
                        }
                    } else {
                        println!("  [DEBUG] prev_hash is not Binary");
                    }
                }
            }
        }
    }

    Ok(None)
}

fn get_height_from_entry(entry_term: &Term) -> Result<Option<u64>> {
    println!("  [DEBUG] get_height_from_entry called");
    // Entry is a PropList with header key
    if let Term::PropList(props) = entry_term {
        println!("  [DEBUG] Entry is PropList with {} props", props.len());
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                if key_bytes == b"header" {
                    println!("  [DEBUG] Found header in entry");
                    match value {
                        Term::Binary(header_bytes) => {
                            println!("  [DEBUG] Header is Binary, decoding...");
                            if let Ok(header_term) = crate::vecpak::decode_term_from_slice(header_bytes) {
                                return get_height_from_header(&header_term);
                            } else {
                                println!("  [DEBUG] Failed to decode header");
                            }
                        }
                        Term::PropList(_) => {
                            println!("  [DEBUG] Header is already PropList");
                            return get_height_from_header(value);
                        }
                        _ => {
                            println!("  [DEBUG] Header is unexpected type");
                        }
                    }
                }
            }
        }
    } else {
        println!("  [DEBUG] Entry is not PropList");
    }

    println!("  [DEBUG] Returning None from get_height_from_entry");
    Ok(None)
}

fn get_height_from_header(header_term: &Term) -> Result<Option<u64>> {
    println!("  [DEBUG] get_height_from_header called");
    if let Term::PropList(props) = header_term {
        println!("  [DEBUG] Header has {} props", props.len());
        for (key, value) in props {
            if let Term::Binary(key_bytes) = key {
                let key_str = String::from_utf8_lossy(key_bytes);
                if key_bytes == b"height" {
                    println!("  [DEBUG] Found height key!");
                    if let Term::VarInt(height) = value {
                        println!("  [DEBUG] Height is VarInt: {}", height);
                        if *height >= 0 {
                            return Ok(Some(*height as u64));
                        }
                    } else {
                        println!("  [DEBUG] Height is not VarInt: {:?}", value);
                    }
                }
            }
        }
    } else {
        println!("  [DEBUG] Header is not PropList");
    }

    println!("  [DEBUG] Returning None from get_height_from_header");
    Ok(None)
}
