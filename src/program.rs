use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;
use object::{Object, ObjectSection, SectionIndex};
use rustc_hash::FxHashMap;
use std::io::Cursor;

pub struct Function {
    section_idx: SectionIndex,
    section_base: u64,

    address: u64,
    length: usize,
}

impl Function {
    pub fn new(section_id: SectionIndex, section_base: u64, address: u64, length: u64) -> Self {
        Self {
            address,
            section_base,
            section_idx: section_id,
            length: length as usize,
        }
    }

    pub fn address(&self) -> u64 {
        self.address
    }
}

pub struct Program {
    pub pointer_size: usize,
    pub functions: FxHashMap<String, Function>,
    pub symbol_map: FxHashMap<u64, String>,
    pub sections: FxHashMap<SectionIndex, Vec<u8>>,
}

impl Program {
    pub fn get_data_for_function(&self, function: &Function) -> Option<&[u8]> {
        let section = self
            .sections
            .get(&function.section_idx)
            .expect("Section Index should never be invalid");

        let relative_address = (function.address - function.section_base) as usize;

        Some(&section[relative_address..relative_address + function.length])
    }

    fn get_section_for_data(
        object: &object::File<'_>,
        address: u64,
    ) -> Option<(u64, SectionIndex)> {
        for section in object.sections() {
            if section.address() > address || (section.address() + section.size()) <= address {
                continue;
            }

            return Some((section.address(), section.index()));
        }

        None
    }

    pub fn load(data: &[u8]) -> Self {
        let object = object::File::parse(data).unwrap();

        let pointer_size = if object.is_64() { 8 } else { 4 };

        let eh_frame = object.section_by_name(".eh_frame").unwrap();
        let eh_frame_data = eh_frame.uncompressed_data().unwrap();

        // FIXME: not that it actually matters, but endian shouldn't be hardcoded
        let fdes = get_fdes::<LittleEndian, _>(
            &mut Cursor::new(eh_frame_data),
            pointer_size,
            eh_frame.address(),
        )
        .unwrap();

        let mut functions: FxHashMap<String, Function> = FxHashMap::default();
        let symbol_map: FxHashMap<u64, String> = object
            .symbol_map()
            .symbols()
            .iter()
            .map(|s| (s.address(), s.name().to_string()))
            .collect();

        let mut sections = FxHashMap::default();
        for fde in fdes {
            if let Some(name) = symbol_map.get(&fde.begin) {
                let (section_base, section_idx) =
                    Self::get_section_for_data(&object, fde.begin).unwrap();

                if !sections.contains_key(&section_idx) {
                    sections.insert(
                        section_idx,
                        object
                            .section_by_index(section_idx)
                            .unwrap()
                            .uncompressed_data()
                            .unwrap()
                            .to_vec(),
                    );
                };

                functions.insert(
                    name.to_string(),
                    Function::new(section_idx, section_base, fde.begin, fde.length),
                );
            } else {
                println!(
                    "function {:08x} (length {:08x}) has no symbol",
                    fde.begin, fde.length
                );
            }
        }

        Self {
            pointer_size,
            functions,
            sections,
            symbol_map,
        }
    }
}
