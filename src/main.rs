mod compare;
mod eh_frame;
mod matcher;
mod program;

use crate::compare::{compare_functions, CompareInfo, CompareResult};
use crate::program::{Function, Program, ProgramInstructionFormatter};

use cpp_demangle::DemangleOptions;
use matcher::FunctionMatcher;

use std::env;
use std::io::IsTerminal;

use rayon::prelude::*;

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

struct FunctionChange {
    info: CompareInfo,
    name: String,
    address1: u64,
    address2: u64,
}

fn print_changes(program1: Box<Program>, program2: Box<Program>, changes: &[FunctionChange]) {
    let mut formatter1 = ProgramInstructionFormatter::new(program1);
    let mut formatter2 = ProgramInstructionFormatter::new(program2);

    for res in changes {
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

        let (instructions1, instructions2) = &res.info.instructions;
        for op in &res.info.diffops {
            match *op {
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

fn get_changes(program1: &Program, program2: &Program) -> Vec<FunctionChange> {
    if program1.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    let matcher = FunctionMatcher::new(program2);
    let matches = program1
        .functions
        .par_iter()
        .filter_map(|(name, func1)| Some((name, func1, matcher.match_name(name)?)));

    let mut changes: Vec<FunctionChange> = matches
        .filter_map(|(name, func1, func2)| {
            match compare_functions(func1, func2, program1.pointer_size) {
                CompareResult::Differs(compare_info) => {
                    let name: String = demangle_symbol(name).unwrap_or(name.to_string());

                    Some(FunctionChange {
                        info: compare_info,
                        name,
                        address1: func1.address,
                        address2: func2.address,
                    })
                }
                CompareResult::Same() => None,
            }
        })
        .collect();

    changes.par_sort_by(|a, b| a.address1.cmp(&b.address1));

    changes
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

    let changes = get_changes(&program1, &program2);
    print_changes(program1, program2, &changes);
}
