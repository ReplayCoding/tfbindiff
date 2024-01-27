use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;
use object::{Object, ObjectSection};
use std::collections::HashMap;
use std::io::Cursor;

pub struct Function {
    address: u64,
    content: Vec<u8>,
}

impl Function {
    pub fn new(address: u64, content: Vec<u8>) -> Self {
        Self { address, content }
    }

    pub fn address(&self) -> u64 {
        self.address
    }

    pub fn content(&self) -> &[u8] {
        &self.content
    }
}

pub struct Program {
    pub pointer_size: usize,
    pub functions: HashMap<String, Function>,
    pub symbol_map: HashMap<u64, String>,
}

impl Program {
    fn get_data_at_address(object: &object::File<'_>, address: u64, size: u64) -> Option<Vec<u8>> {
        for section in object.sections() {
            if section.address() > address || (section.address() + section.size()) <= address {
                continue;
            }

            return if let Ok(section_data) = section.uncompressed_data() {
                let relative_address = address - section.address();
                let data = &section_data[(usize::try_from(relative_address)).ok()?
                    ..usize::try_from(relative_address + size).ok()?];

                Some(data.to_vec())
            } else {
                None
            };
        }

        None
    }

    pub fn load(data: &[u8]) -> Self {
        let object = object::File::parse(data).unwrap();

        let pointer_size = if object.is_64() { 8 } else { 4 };

        let eh_frame = object.section_by_name(".eh_frame").unwrap();
        let eh_frame_data = eh_frame.uncompressed_data().unwrap();

        // FIXME: not that it actually matters, but this shouldn't be hardcoded
        let fdes = get_fdes::<LittleEndian, _>(
            &mut Cursor::new(eh_frame_data),
            pointer_size,
            eh_frame.address(),
        )
        .unwrap();

        let mut functions: HashMap<String, Function> = HashMap::new();
        let symbol_map: HashMap<u64, String> = object
            .symbol_map()
            .symbols()
            .iter()
            .map(|s| (s.address(), s.name().to_string()))
            .collect();

        for fde in fdes {
            if let Some(name) = symbol_map.get(&fde.begin) {
                functions.insert(
                    name.to_string(),
                    Function::new(
                        fde.begin,
                        Self::get_data_at_address(&object, fde.begin, fde.length).unwrap(),
                    ),
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
            symbol_map,
        }
    }
}
