use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;
use memmap2::Mmap;
use object::{Object, ObjectSection};
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;

pub struct Function {
    pub address: u64,
    pub content: Vec<u8>,
}

impl Function {
    pub fn new(address: u64, content: Vec<u8>) -> Function {
        Function { address, content }
    }
}

pub struct Program {
    pub pointer_size: usize,
    pub functions: HashMap<String, Function>,
    pub symbol_map: HashMap<u64, String>,
}

impl Program {
    fn get_data_at_address<'a, O: Object<'a, 'a>>(
        object: &'a O,
        address: u64,
        size: u64,
    ) -> Option<Vec<u8>> {
        for section in object.sections() {
            if section.address() > address || (section.address() + section.size()) <= address {
                continue;
            }

            return if let Ok(section_data) = section.uncompressed_data() {
                let relative_address = address - section.address();
                let data =
                    &section_data[(relative_address as usize)..(relative_address + size) as usize];

                Some(data.to_vec())
            } else {
                None
            };
        }

        None
    }

    pub fn load_path(filename: &str) -> Program {
        let path = Path::new(filename);
        let file = fs::File::open(path).unwrap();
        let buffer = unsafe { Mmap::map(&file).unwrap() };
        let object = object::File::parse(&*buffer).unwrap();

        Program::load(&object)
    }

    pub fn load<'a, O: Object<'a, 'a>>(object: &'a O) -> Program {
        let pointer_size = if object.is_64() { 8 } else { 4 };

        let eh_frame = object.section_by_name(".eh_frame").unwrap();
        let eh_frame_data = eh_frame.uncompressed_data().unwrap();

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
                        Self::get_data_at_address(object, fde.begin, fde.length).unwrap(),
                    ),
                );
            } else {
                println!(
                    "function {:08x} (length {:08x}) has no symbol",
                    fde.begin, fde.length
                );
            }
        }

        Program {
            pointer_size,
            functions,
            symbol_map,
        }
    }
}
