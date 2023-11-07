use crate::instruction_wrapper::{InstructionIter, InstructionWrapper};
use crate::matcher::FunctionMatcher;
use crate::program::{Function, Program};
use iced_x86::{Instruction, Mnemonic, OpKind, Register};

use rayon::prelude::*;

pub enum CompareResult {
    Same(),
    Differs(CompareInfo),
}

pub struct CompareInfo {
    pub diffops: Vec<similar::DiffOp>,
    pub instructions: (Vec<InstructionWrapper>, Vec<InstructionWrapper>),
}

fn get_stack_depth_from_instruction(instr: &Instruction) -> i64 {
    match instr.op1_kind() {
        OpKind::Immediate8to32 => instr.immediate8to32().into(),
        OpKind::Immediate32 => instr.immediate32().into(),
        _ => todo!("stack depth: unhandled op1 type {:?}", instr.op1_kind()),
    }
}

pub fn compare_functions(func1: &Function, func2: &Function, pointer_size: usize) -> CompareResult {
    // If the bytes are the exact same, then there is no difference
    if func1.content == func2.content {
        return CompareResult::Same();
    }

    let mut has_difference = false;

    // New bytes, something was added!
    if func1.content.len() != func2.content.len() {
        has_difference = true;
    }

    let instructions1: Vec<InstructionWrapper> =
        InstructionIter::new(func1.address, &func1.content, pointer_size).collect();
    let instructions2: Vec<InstructionWrapper> =
        InstructionIter::new(func2.address, &func2.content, pointer_size).collect();

    if instructions1 != instructions2 {
        has_difference = true;
    }

    for (instr1, instr2) in std::iter::zip(&instructions1, &instructions2) {
        // Opcode matches, let's check for stack depth
        // FIXME: Only handles 32-bit register
        // sub esp, <depth>
        if instr1.get().mnemonic() == Mnemonic::Sub
            && instr1.get().op0_kind() == OpKind::Register
            && instr1.get().op0_register() == Register::ESP
            && instr2.get().op0_kind() == OpKind::Register
            && instr2.get().op0_register() == Register::ESP
        {
            let stack_depth1: i64 = get_stack_depth_from_instruction(instr1.get());
            let stack_depth2: i64 = get_stack_depth_from_instruction(instr2.get());

            if stack_depth1 != stack_depth2 {
                has_difference = true;
            }

            break;
        }
    }

    if has_difference {
        // NOTE: Lcs panics on oob, wtf?
        let diffops =
            similar::capture_diff_slices(similar::Algorithm::Myers, &instructions1, &instructions2);

        CompareResult::Differs(CompareInfo {
            diffops,
            instructions: (instructions1, instructions2),
        })
    } else {
        CompareResult::Same()
    }
}

pub struct FunctionChange {
    pub info: CompareInfo,
    pub name: String,
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

pub fn compare_programs(program1: &Program, program2: &Program) -> Vec<FunctionChange> {
    if program1.pointer_size != program2.pointer_size {
        panic!("pointer sizes don't match");
    }

    let matcher = FunctionMatcher::new(program2);

    let mut changes: Vec<FunctionChange> = program1
        .functions
        .par_iter()
        .filter_map(|(name, func1)| {
            let func2 = matcher.match_name(name)?;

            match compare_functions(func1, func2, program1.pointer_size) {
                CompareResult::Differs(compare_info) => Some(FunctionChange::new(
                    compare_info,
                    name.to_string(),
                    func1.address,
                    func2.address,
                )),
                CompareResult::Same() => None,
            }
        })
        .collect();

    changes.par_sort_by(|a, b| a.address1.cmp(&b.address1));

    changes
}
