mod eh_frame;

use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;

use object::{Object, ObjectSection};

use std::collections::BTreeMap;
use std::env;
use std::fs;
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

struct Program {
    pub pointer_size: usize,
    pub functions: BTreeMap<String, Function>,
}

struct Function {
    address: u64,

    content: Vec<u8>,
}

impl Function {
    fn new(address: u64, content: Vec<u8>) -> Function {
        Function { address, content }
    }
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
                let mut name: String = symbol.name().to_string();
                if let Some(demangled_name) = demangle_symbol(&name) {
                    name = demangled_name
                };

                functions.insert(
                    name,
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

fn dump_code(address: u64, code: &[u8], address_size: usize) -> Vec<iced_x86::Instruction> {
    let mut decoder = iced_x86::Decoder::with_ip(
        (address_size * 8) as u32,
        code,
        address,
        iced_x86::DecoderOptions::NONE,
    );

    let mut instr = iced_x86::Instruction::default();

    let mut instructions = vec![];

    while decoder.can_decode() {
        decoder.decode_out(&mut instr);
        instructions.push(instr);
    }

    instructions
}

fn compare_functions(func1: &Function, func2: &Function, pointer_size: usize) -> bool {
    // New bytes, something was added!
    if func1.content.len() != func2.content.len() {
        return true;
    }

    // If the bytes are the exact same, then there is no difference
    if func1.content == func2.content {
        return false;
    }

    let code1 = dump_code(func1.address, &func1.content, pointer_size);
    let code2 = dump_code(func2.address, &func2.content, pointer_size);

    let mut info_factory1 = iced_x86::InstructionInfoFactory::new();
    let mut info_factory2 = iced_x86::InstructionInfoFactory::new();
    for (instr1, instr2) in std::iter::zip(code1, code2) {
        let op_code1 = instr1.op_code();
        let op_code2 = instr2.op_code();

        // Opcode doesn't match
        if instr1.code() != instr2.code() {
            return true;
        }

        // Operand count doesn't match
        if op_code1.op_count() != op_code2.op_count() {
            return true;
        }

        for i in 0..op_code1.op_count() {
            let op_kind1 = op_code1.op_kind(i);
            let op_kind2 = op_code2.op_kind(i);

            // Operand kind doesn't match
            if op_kind1 != op_kind2 {
                return true;
            }
        }

        let info1 = info_factory1.info(&instr1);
        let info2 = info_factory2.info(&instr2);

        if info1.used_registers() != info2.used_registers() {
            return true;
        }
    }

    false
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let path = Path::new(args[1].as_str());
    let buffer = fs::read(path).unwrap();
    let object = object::File::parse(&*buffer).unwrap();

    let path2 = Path::new(args[2].as_str());
    let buffer2 = fs::read(path2).unwrap();
    let object2 = object::File::parse(&*buffer2).unwrap();

    let program = Program::load(&object);
    let program2 = Program::load(&object2);

    if program.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    for (name, func1) in program.functions.iter() {
        if let Some(func2) = program2.functions.get(name) {
            if compare_functions(func1, func2, program.pointer_size) {
                println!(
                    "Function {} changed (len {} -> {}) [addr {:08x} -> {:08x}]",
                    name,
                    func1.content.len(),
                    func2.content.len(),
                    func1.address,
                    func2.address
                );
            } else {
                // println!("Function {} same", name);
            };
        }
    }
}
