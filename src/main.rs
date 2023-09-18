mod eh_frame;

use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;

use iced_x86::Formatter;
use object::{Object, ObjectSection};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::Cursor;
use std::path::Path;

fn hexdump(data: &[u8], chunk_size: usize) {
    for chunk in data.chunks(chunk_size) {
        for b in chunk {
            print!("{:02x} ", b);
        }

        // padding to align the ascii display
        for _ in 0..chunk_size - chunk.len() {
            print!("   ");
        }

        for b in chunk {
            let c = *b as char;
            if c.is_alphanumeric() {
                print!("{}", c);
            } else {
                print!(".");
            }
        }

        println!();
    }
}

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;

    sym.demangle(&cpp_demangle::DemangleOptions::new()).ok()
}

struct Function {
    address: u64,

    content: Vec<u8>,
    hash: u64,
}

impl Function {
    fn new(address: u64, content: Vec<u8>) -> Function {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);

        Function {
            address,
            content,
            hash: hasher.finish(),
        }
    }
}

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

fn dump_code(address: u64, code: &[u8], address_size: usize) {
    let address = 0; // HACK: tf2 code is relocatable, so this is fine

    let mut decoder = iced_x86::Decoder::with_ip(
        (address_size * 8) as u32,
        &code,
        address,
        iced_x86::DecoderOptions::NONE,
    );

    let mut formatter = iced_x86::FastFormatter::new();

    let mut output = String::new();
    let mut instruction = iced_x86::Instruction::default();

    while decoder.can_decode() {
        decoder.decode_out(&mut instruction);

        output.clear();
        formatter.format(&instruction, &mut output);

        println!("\t{:016X} {}", instruction.ip(), output);
    }
}

fn main() {
    for (i, arg) in env::args().enumerate() {
        if i == 1 {
            let path = Path::new(arg.as_str());
            let buffer = fs::read(path).unwrap();
            let object = object::File::parse(&*buffer).unwrap();

            let eh_frame = object.section_by_name(".eh_frame").unwrap();
            let eh_frame_data = eh_frame.uncompressed_data().unwrap();

            let pointer_size = if object.is_64() { 8 } else { 4 };

            let fdes = get_fdes::<LittleEndian, _>(
                &mut Cursor::new(eh_frame_data),
                pointer_size,
                eh_frame.address(),
            )
            .unwrap();

            let mut functions: HashMap<String, Function> = HashMap::new();
            let symbol_map = object.symbol_map();
            for fde in fdes {
                if let Some(symbol) = symbol_map.get(fde.begin) {
                    let mut name: String = symbol.name().to_string();
                    if let Some(demangled_name) = demangle_symbol(&name) {
                        name = demangled_name
                    };

                    functions.insert(
                        name,
                        Function::new(
                            fde.begin,
                            get_data_at_address(&object, fde.begin, fde.length).unwrap(),
                        ),
                    );
                } else {
                    println!(
                        "function {:08x} (length {:08x}) has no symbol",
                        fde.begin, fde.length
                    );
                }
            }

            for (name, func) in functions.iter() {
                println!(
                    "function {} (@{:08x}) has length {:08x} [hash {:016x}]",
                    name,
                    func.address,
                    func.content.len(),
                    func.hash,
                );

                dump_code(func.address, &func.content, pointer_size);
            }
        }
    }
}
