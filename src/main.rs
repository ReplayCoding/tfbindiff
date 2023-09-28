mod compare;
mod eh_frame;

use crate::compare::{compare_functions, CompareResult, Function};
use crate::eh_frame::get_fdes;

use byteorder::LittleEndian;

use cpp_demangle::DemangleOptions;
use memmap2::Mmap;
use object::{Object, ObjectSection};

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::Path;

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

struct Program {
    pub pointer_size: usize,
    pub functions: BTreeMap<String, Function>,
}

impl Program {
    // TODO: don't duplicate data
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

    fn load<'a, O: Object<'a, 'a>>(object: &'a O) -> Program {
        let pointer_size = if object.is_64() { 8 } else { 4 };

        let eh_frame = object.section_by_name(".eh_frame").unwrap();
        let eh_frame_data = eh_frame.uncompressed_data().unwrap();

        let fdes = get_fdes::<LittleEndian, _>(
            &mut Cursor::new(eh_frame_data),
            pointer_size,
            eh_frame.address(),
        )
        .unwrap();

        let mut functions: BTreeMap<String, Function> = BTreeMap::new();
        let symbol_map = object.symbol_map();
        for fde in fdes {
            if let Some(symbol) = symbol_map.get(fde.begin) {
                functions.insert(
                    symbol.name().to_string(),
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
        }
    }
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let path = Path::new(args[1].as_str());
    let file = fs::File::open(path).unwrap();
    let buffer = unsafe { Mmap::map(&file).unwrap() };
    let object = object::File::parse(&buffer[..]).unwrap();

    let path2 = Path::new(args[2].as_str());
    let file2 = fs::File::open(path2).unwrap();
    let buffer2 = unsafe { Mmap::map(&file2).unwrap() };
    let object2 = object::File::parse(&*buffer2).unwrap();

    let program = Program::load(&object);
    let program2 = Program::load(&object2);

    if program.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    for (name, func1) in program.functions.iter() {
        if let Some(func2) = program2.functions.get(name) {
            if let CompareResult::Differs(compare_info) =
                compare_functions(func1, func2, program.pointer_size)
            {
                let mut name: String = name.to_string();
                if let Some(demangled_name) = demangle_symbol(&name) {
                    name = demangled_name
                };

                if atty::is(atty::Stream::Stdout) {
                    print!("\x1b[1;36m");
                }

                print!("{}", name);

                if atty::is(atty::Stream::Stdout) {
                    print!("\x1b[0m");
                }

                println!(
                    " changed ({:?}, first change @ {:08x}) [primary {:08x}, secondary {:08x}]",
                    compare_info.difference_types,
                    compare_info.first_difference,
                    func1.address,
                    func2.address
                );
            }
        }
    }
}
