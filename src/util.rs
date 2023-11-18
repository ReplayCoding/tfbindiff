use crate::instruction_wrapper::InstructionWrapper;
use crate::program::Program;
use cpp_demangle::DemangleOptions;
use iced_x86::Formatter;

pub fn demangle_symbol(name: &str) -> Option<String> {
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

pub struct ProgramInstructionFormatter {
    formatter: iced_x86::IntelFormatter,
}

impl ProgramInstructionFormatter {
    pub fn new(program: &'static Program) -> Self {
        Self {
            formatter: iced_x86::IntelFormatter::with_options(
                Some(Box::new(ProgramSymbolResolver { program })),
                None,
            ),
        }
    }

    pub fn format(&mut self, instruction: &InstructionWrapper) -> String {
        let mut out = String::new();
        self.formatter.format(instruction.get(), &mut out);

        out
    }
}
