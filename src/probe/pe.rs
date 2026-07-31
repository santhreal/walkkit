//! PE header parser for Windows executables.

/// Metadata extracted from a PE file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeMetadata {
    /// True if the PE has the DLL characteristic flag.
    pub is_dll: bool,
    /// True if the optional header magic indicates PE32+ (64-bit).
    pub is_64bit: bool,
    /// MD5 imphash of the import table (lowercase `dll.function` sorted).
    pub imphash: String,
    /// Number of sections in the COFF header.
    pub num_sections: u16,
    /// Total number of imported functions across all DLLs.
    pub num_imports: u32,
    /// RVA of the entry point from the optional header.
    pub entry_point_rva: u32,
    /// True if a certificate/security data directory is present.
    pub has_signature: bool,
    /// Names of all sections.
    pub section_names: Vec<String>,
    /// Names of all imported DLLs.
    pub import_dlls: Vec<String>,
}

/// Parse PE headers and extract metadata.
///
/// Returns `None` if the bytes do not constitute a valid PE or if any
/// header read would overflow the buffer.
#[must_use]
pub fn parse_pe(bytes: &[u8]) -> Option<PeMetadata> {
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return None;
    }

    let e_lfanew =
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    if bytes.len() < e_lfanew + 24 || &bytes[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return None;
    }

    let coff_offset = e_lfanew + 4;
    let num_sections = u16::from_le_bytes([bytes[coff_offset + 2], bytes[coff_offset + 3]]);
    let size_optional_header =
        u16::from_le_bytes([bytes[coff_offset + 16], bytes[coff_offset + 17]]);
    let characteristics = u16::from_le_bytes([bytes[coff_offset + 18], bytes[coff_offset + 19]]);

    let optional_header_offset = coff_offset + 20;
    if bytes.len() < optional_header_offset + size_optional_header as usize {
        return None;
    }

    let is_dll = (characteristics & 0x2000) != 0;
    let mut is_64bit = false;
    let mut entry_point_rva = 0u32;
    let mut data_dir_import_rva = 0u32;
    let mut data_dir_import_size = 0u32;
    let mut data_dir_cert_rva = 0u32;
    let mut data_dir_cert_size = 0u32;

    if size_optional_header >= 2 {
        let magic = u16::from_le_bytes([
            bytes[optional_header_offset],
            bytes[optional_header_offset + 1],
        ]);
        is_64bit = magic == 0x20b;
        let pe32 = magic == 0x10b;

        if pe32 || is_64bit {
            if size_optional_header >= 20 {
                entry_point_rva = u32::from_le_bytes([
                    bytes[optional_header_offset + 16],
                    bytes[optional_header_offset + 17],
                    bytes[optional_header_offset + 18],
                    bytes[optional_header_offset + 19],
                ]);
            }

            let data_dir_offset = if is_64bit {
                optional_header_offset + 112
            } else {
                optional_header_offset + 96
            };

            if size_optional_header as usize >= data_dir_offset + 16 - optional_header_offset {
                data_dir_import_rva = read_u32(bytes, data_dir_offset + 8)?;
                data_dir_import_size = read_u32(bytes, data_dir_offset + 12)?;
            }
            if size_optional_header as usize >= data_dir_offset + 40 - optional_header_offset {
                data_dir_cert_rva = read_u32(bytes, data_dir_offset + 32)?;
                data_dir_cert_size = read_u32(bytes, data_dir_offset + 36)?;
            }
        }
    }

    let section_table_offset = optional_header_offset + size_optional_header as usize;
    let section_table_size = num_sections as usize * 40;
    if bytes.len() < section_table_offset + section_table_size {
        return None;
    }

    let mut section_names = Vec::with_capacity(num_sections as usize);
    let mut sections = Vec::with_capacity(num_sections as usize);
    for index in 0..num_sections as usize {
        let section_offset = section_table_offset + index * 40;
        let name_bytes = &bytes[section_offset..section_offset + 8];
        let name_len = name_bytes.iter().position(|&byte| byte == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_bytes[..name_len]).to_string();
        section_names.push(name.clone());

        sections.push(Section {
            name,
            virtual_size: read_u32(bytes, section_offset + 8)?,
            virtual_address: read_u32(bytes, section_offset + 12)?,
            size_of_raw_data: read_u32(bytes, section_offset + 16)?,
            pointer_to_raw_data: read_u32(bytes, section_offset + 20)?,
        });
    }

    let has_signature = data_dir_cert_rva != 0 && data_dir_cert_size != 0;
    let (imphash, num_imports, import_dlls) = compute_imphash(
        bytes,
        &sections,
        data_dir_import_rva,
        data_dir_import_size,
        is_64bit,
    );

    Some(PeMetadata {
        is_dll,
        is_64bit,
        imphash,
        num_sections,
        num_imports,
        entry_point_rva,
        has_signature,
        section_names,
        import_dlls,
    })
}

struct Section {
    #[allow(dead_code)]
    name: String,
    virtual_address: u32,
    virtual_size: u32,
    pointer_to_raw_data: u32,
    size_of_raw_data: u32,
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    if bytes.len() < end {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    if bytes.len() < end {
        return None;
    }
    Some(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

fn rva_to_file_offset(rva: u32, sections: &[Section]) -> Option<usize> {
    for section in sections {
        let limit = section.virtual_size.max(section.size_of_raw_data);
        // Widen the section-end sum to u64: a malformed PE with
        // virtual_address + limit exceeding u32::MAX would otherwise overflow and
        // panic in debug builds. `rva >= virtual_address` is checked first, so the
        // `rva - virtual_address` subtraction cannot underflow, and computing the
        // file offset in usize avoids the pointer_to_raw_data + delta overflow.
        let section_end = section.virtual_address as u64 + limit as u64;
        if rva >= section.virtual_address
            && (rva as u64) < section_end
            && rva - section.virtual_address < section.size_of_raw_data
        {
            return Some(
                section.pointer_to_raw_data as usize + (rva - section.virtual_address) as usize,
            );
        }
    }
    None
}

fn read_null_terminated_string(bytes: &[u8], offset: usize) -> Option<String> {
    if offset >= bytes.len() {
        return None;
    }
    let end = bytes[offset..].iter().position(|&byte| byte == 0)?;
    Some(String::from_utf8_lossy(&bytes[offset..offset + end]).to_string())
}

/// MD5 of the empty string, the imphash of a PE with no resolvable imports.
const EMPTY_MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";

/// Normalize a module name for imphash: everything before the first '.',
/// lowercased. This is the pefile convention (`KERNEL32.dll` -> `kernel32`), so
/// the resulting hash matches pefile/VirusTotal.
fn imphash_module_name(dll_name: &str) -> String {
    dll_name.split('.').next().unwrap_or(dll_name).to_lowercase()
}

/// Compute an imphash from ordered `lib.func` entries. Entry ORDER and DUPLICATES
/// are significant (Mandiant/pefile standard): the entries are comma-joined and
/// MD5-hashed exactly as collected from the import table.
fn imphash_from_entries(entries: &[String]) -> String {
    if entries.is_empty() {
        return EMPTY_MD5.to_string();
    }
    format!("{:x}", md5::compute(entries.join(",").as_bytes()))
}

fn compute_imphash(
    bytes: &[u8],
    sections: &[Section],
    import_rva: u32,
    import_size: u32,
    is_64bit: bool,
) -> (String, u32, Vec<String>) {
    if import_rva == 0 || import_size == 0 || import_size < 20 {
        return (EMPTY_MD5.to_string(), 0, Vec::new());
    }

    let Some(import_dir_offset) = rva_to_file_offset(import_rva, sections) else {
        return (EMPTY_MD5.to_string(), 0, Vec::new());
    };

    // imphash requires entries in IMPORT-TABLE ORDER with duplicates KEPT (the
    // Mandiant/pefile standard), so this must be an ordered Vec, not a sorted,
    // de-duplicating set. Using a BTreeSet here reordered and collapsed entries,
    // producing a hash that never matched pefile/VirusTotal.
    let mut import_entries: Vec<String> = Vec::new();
    let mut import_dlls = Vec::new();
    let mut num_imports = 0u32;

    let max_descriptors = (import_size / 20).min(1024) as usize;
    for index in 0..max_descriptors {
        let descriptor_offset = import_dir_offset + index * 20;
        if bytes.len() < descriptor_offset + 20 {
            break;
        }

        let original_first_thunk = read_u32(bytes, descriptor_offset).unwrap_or(0);
        let name_rva = read_u32(bytes, descriptor_offset + 12).unwrap_or(0);
        let first_thunk = read_u32(bytes, descriptor_offset + 16).unwrap_or(0);

        if original_first_thunk == 0 && name_rva == 0 && first_thunk == 0 {
            break;
        }

        let Some(name_offset) = rva_to_file_offset(name_rva, sections) else {
            continue;
        };
        let dll_name = read_null_terminated_string(bytes, name_offset).unwrap_or_default();
        if dll_name.is_empty() {
            continue;
        }

        import_dlls.push(dll_name.clone());
        let dll_lower = imphash_module_name(&dll_name);
        let thunk_rva = if original_first_thunk != 0 {
            original_first_thunk
        } else {
            first_thunk
        };

        let Some(thunk_offset) = rva_to_file_offset(thunk_rva, sections) else {
            continue;
        };

        let mut thunk_index = 0usize;
        loop {
            let entry_size = if is_64bit { 8 } else { 4 };
            let entry_offset = match thunk_index.checked_mul(entry_size) {
                Some(product) => match thunk_offset.checked_add(product) {
                    Some(offset) => offset,
                    None => break,
                },
                None => break,
            };
            if bytes.len() < entry_offset + entry_size {
                break;
            }

            let entry = if is_64bit {
                read_u64(bytes, entry_offset).unwrap_or(0)
            } else {
                read_u32(bytes, entry_offset).unwrap_or(0) as u64
            };
            if entry == 0 {
                break;
            }

            let ordinal_mask = if is_64bit { 1u64 << 63 } else { 1u64 << 31 };
            if (entry & ordinal_mask) != 0 {
                import_entries.push(format!("{}.ord{}", dll_lower, entry & !ordinal_mask));
                num_imports += 1;
            } else {
                let hint_name_rva = (entry & 0xFFFF_FFFF) as u32;
                if let Some(hint_offset) = rva_to_file_offset(hint_name_rva, sections) {
                    if let Some(function_name) = read_null_terminated_string(bytes, hint_offset + 2)
                    {
                        import_entries.push(format!(
                            "{}.{}",
                            dll_lower,
                            function_name.to_lowercase()
                        ));
                        num_imports += 1;
                    }
                }
            }

            thunk_index += 1;
            if thunk_index > 8192 {
                break;
            }
        }
    }

    let imphash = imphash_from_entries(&import_entries);
    (imphash, num_imports, import_dlls)
}

#[cfg(test)]
mod tests {
    use super::{
        imphash_from_entries, imphash_module_name, parse_pe, rva_to_file_offset, Section, EMPTY_MD5,
    };

    #[test]
    fn imphash_module_name_strips_extension_and_lowercases() {
        assert_eq!(imphash_module_name("KERNEL32.dll"), "kernel32");
        assert_eq!(imphash_module_name("User32.DLL"), "user32");
        assert_eq!(imphash_module_name("ntoskrnl.exe"), "ntoskrnl");
        assert_eq!(imphash_module_name("driver.sys"), "driver");
        // No extension: unchanged except case.
        assert_eq!(imphash_module_name("NTDLL"), "ntdll");
    }

    #[test]
    fn imphash_matches_pefile_reference_string() {
        // The canonical algorithm is md5 of the comma-joined lowercase
        // `module.func` entries in import-table order.
        let entries = vec![
            "kernel32.createfilea".to_string(),
            "user32.messageboxa".to_string(),
        ];
        let expected = format!(
            "{:x}",
            md5::compute(b"kernel32.createfilea,user32.messageboxa")
        );
        assert_eq!(imphash_from_entries(&entries), expected);
    }

    #[test]
    fn imphash_preserves_order_and_duplicates() {
        // Order is significant: swapping entries must change the hash (the old
        // BTreeSet sorted them, collapsing this distinction).
        let ab = vec!["a.f".to_string(), "b.g".to_string()];
        let ba = vec!["b.g".to_string(), "a.f".to_string()];
        assert_ne!(imphash_from_entries(&ab), imphash_from_entries(&ba));
        // Duplicates are kept: `a.f,a.f` != `a.f` (the old set deduped).
        let dup = vec!["a.f".to_string(), "a.f".to_string()];
        let single = vec!["a.f".to_string()];
        assert_ne!(imphash_from_entries(&dup), imphash_from_entries(&single));
    }

    #[test]
    fn imphash_empty_entries_is_md5_of_empty_string() {
        assert_eq!(imphash_from_entries(&[]), EMPTY_MD5);
        assert_eq!(EMPTY_MD5, format!("{:x}", md5::compute(b"")));
    }

    #[test]
    fn rva_to_file_offset_does_not_overflow_on_malformed_section() {
        // A hostile section with virtual_address + size near u32::MAX must not
        // panic (debug overflow) in the section-end computation.
        let sections = vec![Section {
            name: String::new(),
            virtual_address: u32::MAX - 10,
            virtual_size: u32::MAX,
            size_of_raw_data: u32::MAX,
            pointer_to_raw_data: 0,
        }];
        // Just must not panic; the exact result is unimportant here.
        let _ = rva_to_file_offset(u32::MAX - 5, &sections);
        let _ = rva_to_file_offset(0, &sections);
    }

    fn build_minimal_pe() -> Vec<u8> {
        let mut pe = vec![0; 64];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&(0x80u32).to_le_bytes());

        pe.resize(0x80 + 4 + 20 + 0xE0 + 40, 0);
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");

        let coff = 0x84;
        pe[coff + 2..coff + 4].copy_from_slice(&(1u16).to_le_bytes());
        pe[coff + 16..coff + 18].copy_from_slice(&(0xE0u16).to_le_bytes());

        let optional = coff + 20;
        pe[optional..optional + 2].copy_from_slice(&(0x10Bu16).to_le_bytes());
        pe[optional + 16..optional + 20].copy_from_slice(&(0x1000u32).to_le_bytes());

        let section = optional + 0xE0;
        pe[section..section + 5].copy_from_slice(b".text");
        pe[section + 8..section + 12].copy_from_slice(&(0x1000u32).to_le_bytes());
        pe[section + 12..section + 16].copy_from_slice(&(0x1000u32).to_le_bytes());
        pe[section + 16..section + 20].copy_from_slice(&(0x200u32).to_le_bytes());
        pe[section + 20..section + 24].copy_from_slice(&(0x200u32).to_le_bytes());

        pe
    }

    #[test]
    fn rejects_invalid_pe() {
        assert!(parse_pe(&[]).is_none());
        assert!(parse_pe(b"not a pe").is_none());
    }

    #[test]
    fn parses_minimal_pe_headers() {
        let pe = build_minimal_pe();
        let metadata = parse_pe(&pe).expect("minimal PE should parse");
        assert_eq!(metadata.num_sections, 1);
        assert_eq!(metadata.entry_point_rva, 0x1000);
        assert!(!metadata.is_64bit);
        assert!(!metadata.is_dll);
    }

    #[test]
    fn optional_header_size_one_returns_none() {
        let mut pe = vec![0u8; 64];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
        pe.resize(0x80 + 4 + 20 + 1, 0);
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        pe[coff + 2..coff + 4].copy_from_slice(&(1u16).to_le_bytes());
        pe[coff + 16..coff + 18].copy_from_slice(&(1u16).to_le_bytes());

        assert!(parse_pe(&pe).is_none());
    }

    #[test]
    fn optional_header_size_four_returns_none() {
        let mut pe = vec![0u8; 64];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
        pe.resize(0x80 + 4 + 20 + 4, 0);
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        pe[coff + 2..coff + 4].copy_from_slice(&(1u16).to_le_bytes());
        pe[coff + 16..coff + 18].copy_from_slice(&(4u16).to_le_bytes());
        let optional = coff + 20;
        pe[optional..optional + 2].copy_from_slice(&(0x10Bu16).to_le_bytes());

        assert!(parse_pe(&pe).is_none());
    }

    #[test]
    fn parses_pe_with_few_data_directories() {
        let mut pe = build_minimal_pe();
        let coff = 0x84;
        let optional = coff + 20;
        pe[coff + 16..coff + 18].copy_from_slice(&(112u16).to_le_bytes());
        let section_table_start = optional + 112;
        pe.resize(section_table_start + 40, 0);
        pe[section_table_start..section_table_start + 5].copy_from_slice(b".text");
        pe[section_table_start + 8..section_table_start + 12]
            .copy_from_slice(&(0x1000u32).to_le_bytes());
        pe[section_table_start + 12..section_table_start + 16]
            .copy_from_slice(&(0x1000u32).to_le_bytes());
        pe[section_table_start + 16..section_table_start + 20]
            .copy_from_slice(&(0x200u32).to_le_bytes());
        pe[section_table_start + 20..section_table_start + 24]
            .copy_from_slice(&(0x200u32).to_le_bytes());

        let metadata = parse_pe(&pe).expect("PE with few data directories should parse");
        assert!(!metadata.has_signature);
    }

    #[test]
    fn read_u32_u64_checked_overflow() {
        use super::{read_u32, read_u64};
        let bytes = [0u8; 4];
        assert_eq!(read_u32(&bytes, usize::MAX - 3), None);
        assert_eq!(read_u64(&bytes, usize::MAX - 7), None);
        assert_eq!(read_u32(&bytes, 0), Some(0));
    }
}
