mod compare;
mod eh_frame;

use crate::compare::{compare_functions, CompareInfo, CompareResult, Function};
use crate::eh_frame::get_fdes;

use byteorder::LittleEndian;

use cpp_demangle::DemangleOptions;
use memmap2::Mmap;
use object::{Object, ObjectSection};
use once_cell::sync::Lazy;
use regex_lite::Regex;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Cursor;
use std::io::IsTerminal;
use std::path::Path;

use rayon::prelude::*;

static STATIC_INITIALIZER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^_?_GLOBAL__sub_I_(.*)\.stdout\.rel_tf_osx_builder\..*\.ii$").unwrap()
});

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

struct Program {
    pub pointer_size: usize,
    pub functions: HashMap<String, Function>,
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

        let mut functions: HashMap<String, Function> = HashMap::new();
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

fn load_file(filename: &str) -> Program {
    let path = Path::new(filename);
    let file = fs::File::open(path).unwrap();
    let buffer = unsafe { Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*buffer).unwrap();

    Program::load(&object)
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let (program1, program2) = rayon::join(|| load_file(&args[1]), || load_file(&args[2]));

    if program1.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    struct Diff {
        info: CompareInfo,
        name: String,
        address1: u64,
        address2: u64,
    }

    fn build_static_init_map(functions: &HashMap<String, Function>) -> HashMap<String, &String> {
        let mut static_initializers_to_note: HashMap<String, &String> = Default::default();

        let mut static_initializer_blocklist: HashSet<String> = Default::default();
        for (name, _) in functions {
            if let Some(captures) = STATIC_INITIALIZER_REGEX.captures(name) {
                let extracted_filenae = captures.get(1).unwrap().as_str();
                if static_initializer_blocklist.contains(extracted_filenae) {
                    continue;
                }

                if !static_initializers_to_note.contains_key(extracted_filenae) {
                    static_initializers_to_note.insert(extracted_filenae.to_string(), &name);
                } else {
                    static_initializers_to_note.remove(extracted_filenae);
                    static_initializer_blocklist.insert(extracted_filenae.to_string());
                }
            }
        }

        static_initializers_to_note
    }

    let static_init_map2 = build_static_init_map(&program2.functions);

    let matches = program1.functions.par_iter().filter_map(|(name1, func1)| {
        if let Some(func2) = program2.functions.get(name1) {
            Some((name1, func1, func2))
        } else if let Some(captures) = STATIC_INITIALIZER_REGEX.captures(name1) {
            let extracted_filename = &captures[1];
            if let Some(name2) = static_init_map2.get(extracted_filename) {
                if let Some(func2) = program2.functions.get(*name2) {
                    Some((name1, func1, func2))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    });

    let mut diffs: Vec<Diff> = matches
        .filter_map(|(name, func1, func2)| {
            if let CompareResult::Differs(compare_info) =
                compare_functions(func1, func2, program1.pointer_size)
            {
                let mut name: String = name.to_string();
                if let Some(demangled_name) = demangle_symbol(&name) {
                    name = demangled_name
                };

                Some(Diff {
                    info: compare_info,
                    name,
                    address1: func1.address,
                    address2: func2.address,
                })
            } else {
                None
            }
        })
        .collect();

    diffs.par_sort_by(|a, b| a.address1.cmp(&b.address1));

    for res in diffs {
        if std::io::stdout().is_terminal() {
            print!("\x1b[1;36m");
        }

        print!("{}", res.name);

        if std::io::stdout().is_terminal() {
            print!("\x1b[0m");
        }
        println!(
            " changed ({:?}, first change @ {:08x}) [primary {:08x}, secondary {:08x}]",
            res.info.difference_types, res.info.first_difference, res.address1, res.address2
        );
    }
}
