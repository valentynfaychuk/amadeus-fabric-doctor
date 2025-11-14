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
