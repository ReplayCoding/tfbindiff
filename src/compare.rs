use crate::instruction_wrapper::{InstructionIter, InstructionWrapper};
use crate::program::Function;
use iced_x86::{Instruction, Mnemonic, OpKind, Register};

pub enum CompareResult {
    Same(),
    Differs(CompareInfo),
}

#[derive(Debug)]
pub enum DifferenceType {
    FunctionLength,
    DifferentInstruction,
    StackDepth,
}

pub struct CompareInfo {
    pub difference_types: Vec<DifferenceType>,
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

    let mut difference_types: Vec<DifferenceType> = vec![];

    // New bytes, something was added!
    if func1.content.len() != func2.content.len() {
        difference_types.push(DifferenceType::FunctionLength);
    }

    let instructions1: Vec<InstructionWrapper> =
        InstructionIter::new(func1.address, &func1.content, pointer_size).collect();
    let instructions2: Vec<InstructionWrapper> =
        InstructionIter::new(func2.address, &func2.content, pointer_size).collect();

    if instructions1 != instructions2 {
        difference_types.push(DifferenceType::DifferentInstruction);
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
                difference_types.push(DifferenceType::StackDepth);
            }

            break;
        }
    }

    if !difference_types.is_empty() {
        // NOTE: Lcs panics on oob, wtf?
        let diffops =
            similar::capture_diff_slices(similar::Algorithm::Myers, &instructions1, &instructions2);

        CompareResult::Differs(CompareInfo {
            difference_types,
            diffops,
            instructions: (instructions1, instructions2),
        })
    } else {
        CompareResult::Same()
    }
}
