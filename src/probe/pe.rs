//! PE header parser for Windows executables.

use std::collections::BTreeSet;

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

    if size_optional_header > 0 {
        let magic = u16::from_le_bytes([
            bytes[optional_header_offset],
            bytes[optional_header_offset + 1],
        ]);
        is_64bit = magic == 0x20b;
        let pe32 = magic == 0x10b;

        if pe32 || is_64bit {
            entry_point_rva = u32::from_le_bytes([
                bytes[optional_header_offset + 16],
                bytes[optional_header_offset + 17],
                bytes[optional_header_offset + 18],
                bytes[optional_header_offset + 19],
            ]);

            let data_dir_offset = if is_64bit {
                optional_header_offset + 112
            } else {
                optional_header_offset + 96
            };

            if size_optional_header as usize >= data_dir_offset + 16 - optional_header_offset {
                data_dir_import_rva = read_u32(bytes, data_dir_offset + 8)?;
                data_dir_import_size = read_u32(bytes, data_dir_offset + 12)?;
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
    if bytes.len() < offset + 4 {
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
    if bytes.len() < offset + 8 {
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
        if rva >= section.virtual_address
            && rva < section.virtual_address + limit
            && rva - section.virtual_address < section.size_of_raw_data
        {
            return Some((section.pointer_to_raw_data + (rva - section.virtual_address)) as usize);
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

fn compute_imphash(
    bytes: &[u8],
    sections: &[Section],
    import_rva: u32,
    import_size: u32,
    is_64bit: bool,
) -> (String, u32, Vec<String>) {
    let empty_hash = "d41d8cd98f00b204e9800998ecf8427e".to_string();
    if import_rva == 0 || import_size == 0 || import_size < 20 {
        return (empty_hash, 0, Vec::new());
    }

    let Some(import_dir_offset) = rva_to_file_offset(import_rva, sections) else {
        return (empty_hash, 0, Vec::new());
    };

    let mut import_entries = BTreeSet::new();
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
        let dll_lower = dll_name.to_lowercase();
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
            let entry_offset = thunk_offset + thunk_index * entry_size;
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
                import_entries.insert(format!("{}.ord{}", dll_lower, entry & !ordinal_mask));
                num_imports += 1;
            } else {
                let hint_name_rva = (entry & 0xFFFF_FFFF) as u32;
                if let Some(hint_offset) = rva_to_file_offset(hint_name_rva, sections) {
                    if let Some(function_name) = read_null_terminated_string(bytes, hint_offset + 2)
                    {
                        import_entries.insert(format!(
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

    let imphash_input = import_entries.into_iter().collect::<String>();
    let imphash = if imphash_input.is_empty() {
        empty_hash
    } else {
        format!("{:x}", md5::compute(imphash_input.as_bytes()))
    };

    (imphash, num_imports, import_dlls)
}

#[cfg(test)]
mod tests {
    use super::parse_pe;

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
}
