use crate::compare::FunctionChange;
use crate::instruction_wrapper::InstructionWrapper;
use crate::program::Program;
use cpp_demangle::DemangleOptions;
use iced_x86::Formatter;
use std::io::IsTerminal;

fn demangle_symbol(name: &str) -> Option<String> {
    let sym = cpp_demangle::Symbol::new(name).ok()?;
    let options = DemangleOptions::new().no_params();

    sym.demangle(&options).ok()
}

struct ProgramSymbolResolver {
    // Why does this have a static lifetime? Because the iced formatter api is stupid and takes an
    // owned box, instead of a reference.
    program: &'static Program,
}

impl iced_x86::SymbolResolver for ProgramSymbolResolver {
    fn symbol(
        &mut self,
        _instruction: &iced_x86::Instruction,
        _operand: u32,
        _instruction_operand: Option<u32>,
        address: u64,
        _address_size: u32,
    ) -> Option<iced_x86::SymbolResult<'_>> {
        let mangled_name = self.program.symbol_map.get(&address)?;
        let name = demangle_symbol(mangled_name).unwrap_or(mangled_name.clone());

        Some(iced_x86::SymbolResult::with_string(address, name))
    }
}

struct ProgramInstructionFormatter {
    formatter: iced_x86::IntelFormatter,
}

impl ProgramInstructionFormatter {
    fn new(program: &'static Program) -> Self {
        Self {
            formatter: iced_x86::IntelFormatter::with_options(
                Some(Box::new(ProgramSymbolResolver { program })),
                None,
            ),
        }
    }

    fn format(&mut self, instructions: &[InstructionWrapper]) -> Vec<String> {
        let mut formatted_instructions = vec![];
        formatted_instructions.reserve(instructions.len());

        for instruction in instructions {
            let mut out = String::new();
            self.formatter.format(instruction.get(), &mut out);

            formatted_instructions.push(out);
        }

        formatted_instructions
    }
}

pub fn print_changes(
    program1: &'static Program,
    program2: &'static Program,
    changes: &[FunctionChange],
) {
    let mut formatter1 = ProgramInstructionFormatter::new(program1);
    let mut formatter2 = ProgramInstructionFormatter::new(program2);

    for res in changes {
        if std::io::stdout().is_terminal() {
            print!("\x1b[1;36m");
        }

        let name: String = demangle_symbol(res.name()).unwrap_or(res.name().to_string());
        print!("{}", name);

        if std::io::stdout().is_terminal() {
            print!("\x1b[0m");
        }

        println!(
            " changed [primary {:08x}, secondary {:08x}]",
            res.address1(),
            res.address2()
        );

        let (instructions1, instructions2) = res.instructions();
        for op in res.diff_ops() {
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
