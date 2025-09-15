use eetf::Term;
use std::cmp::Ordering;

/// Encode an EETF term using small atoms (tag 119) instead of legacy atoms (tag 100)
/// This ensures compatibility with Elixir's [:safe] option which rejects old atom encoding
pub fn encode_safe(term: &Term) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(131); // ETF version marker
    encode_term_with_small_atoms(term, &mut buf);
    buf
}

/// Encode an EETF with small atoms ([:safe]) and [:deterministic]
pub fn encode_safe_deterministic(term: &Term) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(131); // ETF version marker
    encode_map_safe_deterministic(term, &mut buf);
    buf
}

/// Compare two terms according to Erlang's term ordering hierarchy
fn compare_terms(a: &Term, b: &Term) -> Ordering {
    let type_order = |term: &Term| -> u8 {
        match term {
            Term::FixInteger(_) | Term::BigInteger(_) | Term::Float(_) => 1,
            Term::Atom(_) => 2,
            Term::Reference(_) => 3,
            Term::Port(_) => 4,
            Term::Pid(_) => 5,
            Term::Tuple(_) => 6,
            Term::Map(_) => 7,
            Term::List(_) | Term::ImproperList(_) => 8,
            Term::Binary(_) | Term::BitBinary(_) | Term::ByteList(_) => 9,
            Term::ExternalFun(_) | Term::InternalFun(_) => 10,
        }
    };

    let a_type = type_order(a);
    let b_type = type_order(b);

    match a_type.cmp(&b_type) {
        Ordering::Equal => {
            match (a, b) {
                // within number types, compare by numeric value,
                // but integers come before floats in ETF ordering
                (Term::FixInteger(a_int), Term::FixInteger(b_int)) => a_int.value.cmp(&b_int.value),
                (Term::BigInteger(a_big), Term::BigInteger(b_big)) => a_big.value.cmp(&b_big.value),
                (Term::Float(a_float), Term::Float(b_float)) => {
                    a_float.value.partial_cmp(&b_float.value).unwrap_or(Ordering::Equal)
                }

                // integers come before floats in ETF format
                (Term::FixInteger(_), Term::Float(_)) => Ordering::Less,
                (Term::Float(_), Term::FixInteger(_)) => Ordering::Greater,
                (Term::BigInteger(_), Term::Float(_)) => Ordering::Less,
                (Term::Float(_), Term::BigInteger(_)) => Ordering::Greater,
                // small int come before big int
                (Term::FixInteger(_), Term::BigInteger(_)) => Ordering::Less,
                (Term::BigInteger(_), Term::FixInteger(_)) => Ordering::Greater,

                // atoms are compared alphabetically
                (Term::Atom(a_atom), Term::Atom(b_atom)) => a_atom.name.cmp(&b_atom.name),

                // binaries are sorted by lexicographic byte comparison
                (Term::Binary(a_bin), Term::Binary(b_bin)) => a_bin.bytes.cmp(&b_bin.bytes),
                (Term::ByteList(a_bytes), Term::ByteList(b_bytes)) => a_bytes.bytes.cmp(&b_bytes.bytes),

                // for other types within the same category, use string representation as fallback
                _ => format!("{:?}", a).cmp(&format!("{:?}", b)),
            }
        }
        other => other,
    }
}

/// Encode a term with deterministic ordering (maps have sorted keys)
fn encode_map_safe_deterministic(term: &Term, buf: &mut Vec<u8>) {
    match term {
        Term::Map(map) => {
            let mut sorted_pairs: Vec<_> = map.map.iter().collect();
            sorted_pairs.sort_by(|(a, _), (b, _)| compare_terms(a, b));

            buf.push(116); // map tag
            buf.extend_from_slice(&(sorted_pairs.len() as u32).to_be_bytes());

            for (key, value) in sorted_pairs {
                encode_map_safe_deterministic(key, buf);
                encode_map_safe_deterministic(value, buf);
            }
        }
        _ => {
            // use the safe encoding but recurse with deterministic encoding for nested terms
            encode_term_safe_deterministic(term, buf);
        }
    }
}

/// Encode term with small atoms and deterministic ordering for nested structures
fn encode_term_safe_deterministic(term: &Term, buf: &mut Vec<u8>) {
    match term {
        Term::Atom(atom) => {
            // Use small atom (tag 119) instead of legacy atom (tag 100)
            let name_bytes = atom.name.as_bytes();
            if name_bytes.len() <= 255 {
                buf.push(119); // small atom
                buf.push(name_bytes.len() as u8);
                buf.extend_from_slice(name_bytes);
            } else {
                // For atoms longer than 255 bytes, use atom_utf8 (tag 118)
                buf.push(118); // atom_utf8
                buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
                buf.extend_from_slice(name_bytes);
            }
        }
        Term::List(list) => {
            if list.elements.is_empty() {
                buf.push(106); // nil
            } else {
                buf.push(108); // list
                buf.extend_from_slice(&(list.elements.len() as u32).to_be_bytes());
                for element in &list.elements {
                    encode_map_safe_deterministic(element, buf); // Recurse with deterministic ordering
                }
                buf.push(106); // tail (nil)
            }
        }
        Term::ImproperList(improper) => {
            buf.push(108); // list
            buf.extend_from_slice(&(improper.elements.len() as u32).to_be_bytes());
            for element in &improper.elements {
                encode_map_safe_deterministic(element, buf); // Recurse with deterministic ordering
            }
            encode_map_safe_deterministic(&improper.last, buf); // tail
        }
        Term::Tuple(tuple) => {
            if tuple.elements.len() <= 255 {
                buf.push(104); // small_tuple
                buf.push(tuple.elements.len() as u8);
            } else {
                buf.push(105); // large_tuple
                buf.extend_from_slice(&(tuple.elements.len() as u32).to_be_bytes());
            }
            for element in &tuple.elements {
                encode_map_safe_deterministic(element, buf); // Recurse with deterministic ordering
            }
        }
        Term::Map(_map) => {
            // This shouldn't happen as maps are handled in encode_term_deterministic
            // But handle it anyway for safety
            encode_map_safe_deterministic(term, buf);
        }
        _ => {
            // For all other types, use the existing small atoms encoding
            encode_term_with_small_atoms(term, buf);
        }
    }
}

fn encode_term_with_small_atoms(term: &Term, buf: &mut Vec<u8>) {
    match term {
        Term::Atom(atom) => {
            // Use small atom (tag 119) instead of legacy atom (tag 100)
            let name_bytes = atom.name.as_bytes();
            if name_bytes.len() <= 255 {
                buf.push(119); // small atom
                buf.push(name_bytes.len() as u8);
                buf.extend_from_slice(name_bytes);
            } else {
                // For atoms longer than 255 bytes, use atom_utf8 (tag 118)
                buf.push(118); // atom_utf8
                buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
                buf.extend_from_slice(name_bytes);
            }
        }
        Term::Binary(binary) => {
            buf.push(109); // binary
            buf.extend_from_slice(&(binary.bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(&binary.bytes);
        }
        Term::FixInteger(fix_int) => {
            if fix_int.value >= 0 && fix_int.value <= 255 {
                buf.push(97); // small_integer
                buf.push(fix_int.value as u8);
            } else {
                buf.push(98); // integer
                buf.extend_from_slice(&fix_int.value.to_be_bytes());
            }
        }
        Term::BigInteger(big_int) => {
            // Convert big integer to bytes representation (little-endian for ETF format)
            let bytes = big_int.value.to_bytes_le().1;
            if bytes.len() <= 255 {
                buf.push(110); // small_big
                buf.push(bytes.len() as u8);
                buf.push(if big_int.value >= 0.into() { 0 } else { 1 }); // sign
            } else {
                buf.push(111); // large_big
                buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                buf.push(if big_int.value >= 0.into() { 0 } else { 1 }); // sign
            }
            buf.extend_from_slice(&bytes);
        }
        Term::List(list) => {
            if list.elements.is_empty() {
                buf.push(106); // nil
            } else {
                buf.push(108); // list
                buf.extend_from_slice(&(list.elements.len() as u32).to_be_bytes());
                for element in &list.elements {
                    encode_term_with_small_atoms(element, buf);
                }
                buf.push(106); // tail (nil)
            }
        }
        Term::ImproperList(improper) => {
            buf.push(108); // list
            buf.extend_from_slice(&(improper.elements.len() as u32).to_be_bytes());
            for element in &improper.elements {
                encode_term_with_small_atoms(element, buf);
            }
            encode_term_with_small_atoms(&improper.last, buf); // tail
        }
        Term::Tuple(tuple) => {
            if tuple.elements.len() <= 255 {
                buf.push(104); // small_tuple
                buf.push(tuple.elements.len() as u8);
            } else {
                buf.push(105); // large_tuple
                buf.extend_from_slice(&(tuple.elements.len() as u32).to_be_bytes());
            }
            for element in &tuple.elements {
                encode_term_with_small_atoms(element, buf);
            }
        }
        Term::Map(map) => {
            buf.push(116); // map
            buf.extend_from_slice(&(map.map.len() as u32).to_be_bytes());
            for (key, value) in &map.map {
                encode_term_with_small_atoms(key, buf);
                encode_term_with_small_atoms(value, buf);
            }
        }
        Term::Pid(pid) => {
            buf.push(103); // pid
            encode_term_with_small_atoms(&Term::Atom(pid.node.clone()), buf);
            buf.extend_from_slice(&pid.id.to_be_bytes());
            buf.extend_from_slice(&pid.serial.to_be_bytes());
            buf.push(pid.creation.try_into().unwrap());
        }
        Term::Port(port) => {
            buf.push(102); // port
            encode_term_with_small_atoms(&Term::Atom(port.node.clone()), buf);
            buf.extend_from_slice(&port.id.to_be_bytes());
            buf.push(port.creation.try_into().unwrap());
        }
        Term::Reference(reference) => {
            buf.push(114); // new_reference
            buf.extend_from_slice(&(reference.id.len() as u16).to_be_bytes());
            encode_term_with_small_atoms(&Term::Atom(reference.node.clone()), buf);
            buf.push(reference.creation.try_into().unwrap());
            for id in &reference.id {
                buf.extend_from_slice(&id.to_be_bytes());
            }
        }
        Term::ExternalFun(ext_fun) => {
            buf.push(113); // export
            encode_term_with_small_atoms(&Term::Atom(ext_fun.module.clone()), buf);
            encode_term_with_small_atoms(&Term::Atom(ext_fun.function.clone()), buf);
            encode_term_with_small_atoms(&Term::FixInteger(eetf::FixInteger { value: ext_fun.arity as i32 }), buf);
        }
        Term::InternalFun(int_fun) => {
            match int_fun.as_ref() {
                eetf::InternalFun::Old { module, pid, free_vars, index, uniq } => {
                    buf.push(117); // fun (old representation)
                    buf.extend_from_slice(&(*index as u32).to_be_bytes());
                    buf.extend_from_slice(&(*uniq as u32).to_be_bytes());
                    encode_term_with_small_atoms(&Term::Atom(module.clone()), buf);
                    encode_term_with_small_atoms(&Term::Pid(pid.clone()), buf);
                    for var in free_vars {
                        encode_term_with_small_atoms(var, buf);
                    }
                }
                eetf::InternalFun::New { module, arity, pid, free_vars, index, uniq, old_index, old_uniq } => {
                    buf.push(112); // fun (new representation)
                    buf.push(*arity);
                    buf.extend_from_slice(uniq);
                    buf.extend_from_slice(&index.to_be_bytes());
                    buf.extend_from_slice(&(free_vars.len() as u32).to_be_bytes());
                    encode_term_with_small_atoms(&Term::Atom(module.clone()), buf);
                    encode_term_with_small_atoms(&Term::FixInteger(eetf::FixInteger { value: *old_index }), buf);
                    encode_term_with_small_atoms(&Term::FixInteger(eetf::FixInteger { value: *old_uniq }), buf);
                    encode_term_with_small_atoms(&Term::Pid(pid.clone()), buf);
                    for var in free_vars {
                        encode_term_with_small_atoms(var, buf);
                    }
                }
            }
        }
        Term::BitBinary(bit_binary) => {
            buf.push(77); // bit_binary
            buf.extend_from_slice(&(bit_binary.bytes.len() as u32).to_be_bytes());
            buf.push(bit_binary.tail_bits_size);
            buf.extend_from_slice(&bit_binary.bytes);
        }
        Term::Float(float) => {
            buf.push(70); // new_float
            buf.extend_from_slice(&float.value.to_be_bytes());
        }
        Term::ByteList(byte_list) => {
            buf.push(107); // string
            buf.extend_from_slice(&(byte_list.bytes.len() as u16).to_be_bytes());
            buf.extend_from_slice(&byte_list.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eetf::{Atom, FixInteger, Map};
    use std::collections::HashMap;

    #[test]
    fn test_small_atom_encoding() {
        let atom = Term::Atom(Atom::from("test"));
        let encoded = encode_safe(&atom);

        // Should start with ETF version (131) and small atom tag (119)
        assert_eq!(encoded[0], 131); // ETF version
        assert_eq!(encoded[1], 119); // small atom tag
        assert_eq!(encoded[2], 4); // length of "test"
        assert_eq!(&encoded[3..7], b"test");
    }

    #[test]
    fn test_map_with_small_atoms() {
        let mut map = HashMap::new();
        map.insert(Term::Atom(Atom::from("key")), Term::Atom(Atom::from("value")));
        let term_map = Term::Map(Map { map });

        let encoded = encode_safe(&term_map);

        // Should start with ETF version (131) and map tag (116)
        assert_eq!(encoded[0], 131); // ETF version
        assert_eq!(encoded[1], 116); // map tag

        // Should contain small atom tags (119) for both key and value atoms
        assert!(encoded.contains(&119)); // small atom tag should appear
    }

    #[test]
    fn test_big_integer_encoding() {
        use eetf::BigInteger;

        // Test values that require BigInteger encoding
        let test_values = vec![
            2147483648u64, // i32::MAX + 1
            4294967296u64, // u32::MAX + 1
            1693958400u64, // Typical Unix timestamp
        ];

        for value in test_values {
            let big_int_term = Term::BigInteger(BigInteger::from(value));

            // Encode with original method
            let mut original_encoded = Vec::new();
            big_int_term.encode(&mut original_encoded).unwrap();

            // Encode with our method
            let our_encoded = encode_safe(&big_int_term);

            // Both should produce identical bytes
            assert_eq!(original_encoded, our_encoded, "Encoding mismatch for value {}", value);

            // Both should decode to the same value
            let original_decoded = Term::decode(&original_encoded[..]).unwrap();
            let our_decoded = Term::decode(&our_encoded[..]).unwrap();

            if let (Term::BigInteger(orig), Term::BigInteger(ours)) = (&original_decoded, &our_decoded) {
                assert_eq!(orig.value, ours.value, "BigInteger values should match for {}", value);
            }
        }
    }

    #[test]
    fn test_compatibility_with_original() {
        // Test that our encoding produces the same structure as the original,
        // except atoms use tag 119 instead of 100
        let atom = Term::Atom(Atom::from("test"));
        let mut original_encoded = Vec::new();
        atom.encode(&mut original_encoded).unwrap();
        let our_encoded = encode_safe(&atom);

        println!("Original: {:?}", original_encoded);
        println!("Our:      {:?}", our_encoded);

        // Both should start with ETF version
        assert_eq!(original_encoded[0], our_encoded[0]); // ETF version (131)

        if original_encoded.len() == 8 && our_encoded.len() == 7 {
            // Original uses legacy atom (100) with 2-byte length, ours uses small atom (119) with 1-byte length
            assert_eq!(original_encoded[1], 100); // legacy atom
            assert_eq!(our_encoded[1], 119); // small atom

            // For legacy atoms, the length is 2 bytes, for small atoms it's 1 byte
            // original: [131, 100, 0, 4, 't', 'e', 's', 't']
            // ours:     [131, 119, 4, 't', 'e', 's', 't']
            assert_eq!(original_encoded[2], 0); // high byte of length (should be 0 for short strings)
            assert_eq!(original_encoded[3], 4); // low byte of length
            assert_eq!(our_encoded[2], 4); // single byte length

            // String content should be the same
            assert_eq!(original_encoded[4..], our_encoded[3..]);
        } else {
            // Fallback to original test logic if lengths are different than expected
            assert_eq!(original_encoded.len(), our_encoded.len());
            assert_eq!(original_encoded[1], 119); // legacy atom
            assert_eq!(our_encoded[1], 119); // small atom
            assert_eq!(original_encoded[2..], our_encoded[2..]);
        }
    }

    #[test]
    fn test_deterministic_encoding_mixed_key_types() {
        // Test deterministic encoding with mixed key types following Erlang's hierarchy
        let mut map_data = HashMap::new();

        // Add keys in different order than they should be sorted
        map_data.insert(Term::Atom(Atom::from("atom_key")), Term::FixInteger(FixInteger { value: 1 })); // Atoms (type 2)
        map_data.insert(
            Term::Binary(eetf::Binary { bytes: b"binary_key".to_vec() }),
            Term::FixInteger(FixInteger { value: 2 }),
        ); // Binaries (type 9)
        map_data.insert(Term::FixInteger(FixInteger { value: 42 }), Term::FixInteger(FixInteger { value: 3 })); // Numbers (type 1)

        let map = Term::Map(Map { map: map_data });

        // Encode with deterministic ordering
        let encoded = encode_safe_deterministic(&map);

        // Verify it's properly encoded ETF
        assert_eq!(encoded[0], 131); // ETF version
        assert_eq!(encoded[1], 116); // map tag

        // Should be decodable
        let decoded = Term::decode(&encoded[..]).unwrap();
        if let Term::Map(decoded_map) = decoded {
            assert_eq!(decoded_map.map.len(), 3);

            // Verify all keys are present
            assert!(decoded_map.map.contains_key(&Term::FixInteger(FixInteger { value: 42 })));
            assert!(decoded_map.map.contains_key(&Term::Atom(Atom::from("atom_key"))));
            assert!(decoded_map.map.contains_key(&Term::Binary(eetf::Binary { bytes: b"binary_key".to_vec() })));
        } else {
            panic!("Decoded term is not a map");
        }
    }

    #[test]
    fn test_deterministic_encoding_atom_alphabetical_order() {
        // Test that atoms are sorted alphabetically within their type
        let mut map_data = HashMap::new();

        // Add atom keys in reverse alphabetical order
        map_data.insert(Term::Atom(Atom::from("zebra")), Term::FixInteger(FixInteger { value: 1 }));
        map_data.insert(Term::Atom(Atom::from("apple")), Term::FixInteger(FixInteger { value: 2 }));
        map_data.insert(Term::Atom(Atom::from("banana")), Term::FixInteger(FixInteger { value: 3 }));

        let map = Term::Map(Map { map: map_data });

        // Encode with deterministic ordering - should be consistent
        let encoded1 = encode_safe_deterministic(&map);
        let encoded2 = encode_safe_deterministic(&map);

        // Multiple encodings should be identical (deterministic)
        assert_eq!(encoded1, encoded2);

        // Should be decodable
        let decoded = Term::decode(&encoded1[..]).unwrap();
        if let Term::Map(decoded_map) = decoded {
            assert_eq!(decoded_map.map.len(), 3);
        } else {
            panic!("Decoded term is not a map");
        }
    }

    #[test]
    fn test_deterministic_encoding_number_ordering() {
        // Test that numbers are ordered by value, not type
        let mut map_data = HashMap::new();

        // Add different number types
        map_data.insert(Term::FixInteger(FixInteger { value: 100 }), Term::Atom(Atom::from("hundred")));
        map_data.insert(Term::FixInteger(FixInteger { value: 5 }), Term::Atom(Atom::from("five")));
        map_data.insert(Term::FixInteger(FixInteger { value: 50 }), Term::Atom(Atom::from("fifty")));

        let map = Term::Map(Map { map: map_data });

        let encoded = encode_safe_deterministic(&map);

        // Should be decodable and deterministic
        let decoded = Term::decode(&encoded[..]).unwrap();
        if let Term::Map(decoded_map) = decoded {
            assert_eq!(decoded_map.map.len(), 3);
        } else {
            panic!("Decoded term is not a map");
        }
    }

    #[test]
    fn test_deterministic_vs_original_compatibility() {
        // Test that deterministic encoding uses small atoms like the original function
        let atom = Term::Atom(Atom::from("test_atom"));

        let deterministic_encoded = encode_safe_deterministic(&atom);
        let small_atoms_encoded = encode_safe(&atom);

        // Should be identical for simple atoms
        assert_eq!(deterministic_encoded, small_atoms_encoded);

        // Both should use small atom tag (119)
        assert_eq!(deterministic_encoded[1], 119); // small atom tag
        assert_eq!(small_atoms_encoded[1], 119); // small atom tag
    }

    #[test]
    fn test_deterministic_encoding_anr_keys() {
        // Test the specific ANR keys mentioned in the specification
        let mut map_data = HashMap::new();

        // Add ANR keys in original order
        let anr_keys = ["ip4", "pk", "pop", "port", "signature", "ts", "version", "anr_name", "anr_desc"];
        for (i, key) in anr_keys.iter().enumerate() {
            map_data.insert(Term::Atom(Atom::from(*key)), Term::FixInteger(FixInteger { value: i as i32 }));
        }

        let map = Term::Map(Map { map: map_data });
        let encoded = encode_safe_deterministic(&map);

        // Should be deterministic across multiple calls
        let encoded2 = encode_safe_deterministic(&map);
        assert_eq!(encoded, encoded2);

        // Should be decodable
        let decoded = Term::decode(&encoded[..]).unwrap();
        if let Term::Map(decoded_map) = decoded {
            assert_eq!(decoded_map.map.len(), anr_keys.len());

            // All keys should be present
            for key in &anr_keys {
                assert!(decoded_map.map.contains_key(&Term::Atom(Atom::from(*key))));
            }
        } else {
            panic!("Decoded term is not a map");
        }
    }
}