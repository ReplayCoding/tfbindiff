mod compare;
mod eh_frame;
mod program;

use crate::compare::{compare_functions, CompareInfo, CompareResult};
use crate::program::{Function, Program, ProgramInstructionFormatter};

use cpp_demangle::DemangleOptions;
use once_cell::sync::Lazy;
use owo_colors::OwoColorize;
use regex_lite::Regex;

use std::collections::{HashMap, HashSet};
use std::env;

use rayon::prelude::*;

static STATIC_INITIALIZER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^_?_GLOBAL__sub_I_(.*)\.stdout\.rel_tf_osx_builder\..*\.ii$").unwrap()
});

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let (program1, program2) = rayon::join(
        || Box::new(Program::load_path(&args[1])),
        || Box::new(Program::load_path(&args[2])),
    );

    if program1.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    struct FunctionChange {
        info: CompareInfo,
        name: String,
        address1: u64,
        address2: u64,
    }

    fn build_static_init_map(functions: &HashMap<String, Function>) -> HashMap<String, &String> {
        let mut static_initializers_to_note: HashMap<String, &String> = Default::default();

        let mut static_initializer_blocklist: HashSet<String> = Default::default();
        for name in functions.keys() {
            if let Some(captures) = STATIC_INITIALIZER_REGEX.captures(name) {
                let extracted_filenae = captures.get(1).unwrap().as_str();
                if static_initializer_blocklist.contains(extracted_filenae) {
                    continue;
                }

                if !static_initializers_to_note.contains_key(extracted_filenae) {
                    static_initializers_to_note.insert(extracted_filenae.to_string(), name);
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
                program2
                    .functions
                    .get(*name2)
                    .map(|func2| (name1, func1, func2))
            } else {
                None
            }
        } else {
            None
        }
    });

    let mut changes: Vec<FunctionChange> = matches
        .filter_map(|(name, func1, func2)| {
            if let CompareResult::Differs(compare_info) =
                compare_functions(func1, func2, program1.pointer_size)
            {
                let mut name: String = name.to_string();
                if let Some(demangled_name) = demangle_symbol(&name) {
                    name = demangled_name
                };

                Some(FunctionChange {
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

    changes.par_sort_by(|a, b| a.address1.cmp(&b.address1));

    let mut formatter1 = ProgramInstructionFormatter::new(program1);
    let mut formatter2 = ProgramInstructionFormatter::new(program2);

    for res in changes {
        println!(
            "{} changed ({:?}, first change @ {:08x}) [primary {:08x}, secondary {:08x}]",
            res.name
                .if_supports_color(owo_colors::Stream::Stdout, |n| n.cyan()),
            res.info.difference_types,
            res.info.first_difference,
            res.address1,
            res.address2
        );

        let (instructions1, instructions2) = &res.info.instructions;
        for op in res.info.diffops {
            match op {
                similar::DiffOp::Equal {
                    old_index: _,
                    new_index: _,
                    len: _,
                } => continue,
                similar::DiffOp::Delete {
                    old_index,
                    old_len,
                    new_index,
                } => {
                    println!("deleted old {:08X} new {:08X}", old_index, new_index);

                    for i in formatter1.format(&instructions1[old_index..old_index + old_len]) {
                        println!("\t- {}", i);
                    }
                }
                similar::DiffOp::Insert {
                    old_index,
                    new_index,
                    new_len,
                } => {
                    println!("inserted new {:08X} old {:08X}", new_index, old_index);

                    for i in formatter2.format(&instructions2[new_index..new_index + new_len]) {
                        println!("\t+ {}", i);
                    }
                }
                similar::DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    println!(
                        "replace old {:08X} len {:08X} new {:08X} len {:08X}",
                        new_index, new_len, old_index, old_len
                    );

                    for i in formatter1.format(&instructions1[old_index..old_index + old_len]) {
                        println!("\t- {}", i);
                    }

                    for i in formatter2.format(&instructions2[new_index..new_index + new_len]) {
                        println!("\t+ {}", i);
                    }
                }
            }

            println!();
        }
    }
}
