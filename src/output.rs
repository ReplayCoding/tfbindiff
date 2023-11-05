use crate::compare::CompareInfo;
use crate::program::{Program, ProgramInstructionFormatter};
use cpp_demangle::DemangleOptions;
use std::io::IsTerminal;

pub fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

pub struct FunctionChange {
    info: CompareInfo,
    name: String,
    // HACK
    pub address1: u64,
    pub address2: u64,
}

impl FunctionChange {
    pub fn new(info: CompareInfo, name: String, address1: u64, address2: u64) -> Self {
        Self {
            info,
            name,
            address1,
            address2,
        }
    }
}

pub fn print_changes(program1: Box<Program>, program2: Box<Program>, changes: &[FunctionChange]) {
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
            " changed ({:?}) [primary {:08x}, secondary {:08x}]",
            res.info.difference_types, res.address1, res.address2
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
                    println!(
                        "deleted old {:08x} new {:08x}",
                        &instructions1[old_index].get().ip(),
                        &instructions2[new_index].get().ip()
                    );

                    for i in formatter1.format(&instructions1[old_index..old_index + old_len]) {
                        println!("\t- {}", i);
                    }
                }
                similar::DiffOp::Insert {
                    old_index,
                    new_index,
                    new_len,
                } => {
                    println!(
                        "insert old {:08x} new {:08x}",
                        &instructions1[old_index].get().ip(),
                        &instructions2[new_index].get().ip()
                    );

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
                        "insert old {:08x} new {:08x}",
                        &instructions1[old_index].get().ip(),
                        &instructions2[new_index].get().ip()
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
