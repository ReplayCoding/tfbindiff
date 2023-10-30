use crate::Function;
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use std::hash::Hash;

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
    pub first_difference: u64,
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

#[derive(Clone, Copy)]
pub struct InstructionWrapper(Instruction);
impl InstructionWrapper {
    pub fn get(&self) -> &Instruction {
        &self.0
    }
}

impl Eq for InstructionWrapper {}
impl PartialEq for InstructionWrapper {
    fn eq(&self, other: &Self) -> bool {
        (self.0.code() == other.0.code())
            && (self.0.op_code().op_kinds() == other.0.op_code().op_kinds())
    }
}

impl Hash for InstructionWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.code().hash(state);
        self.0.op_code().op_kinds().hash(state);
    }
}

impl Ord for InstructionWrapper {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        // For some reason this is required to diff, but not used??
        todo!("implement Ord for instructions")
    }
}

impl PartialOrd for InstructionWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct InstructionIter<'a> {
    decoder: Decoder<'a>,
}

impl<'a> InstructionIter<'a> {
    fn new(address: u64, code: &'a [u8], address_size: usize) -> Self {
        Self {
            decoder: Decoder::with_ip(
                (address_size * 8) as u32,
                code,
                address,
                DecoderOptions::NONE,
            ),
        }
    }
}

impl<'a> Iterator for InstructionIter<'a> {
    type Item = InstructionWrapper;

    fn next(&mut self) -> Option<Self::Item> {
        if self.decoder.can_decode() {
            Some(InstructionWrapper(self.decoder.decode()))
        } else {
            None
        }
    }
}

pub fn compare_functions(func1: &Function, func2: &Function, pointer_size: usize) -> CompareResult {
    // If the bytes are the exact same, then there is no difference
    if func1.content == func2.content {
        return CompareResult::Same();
    }

    let mut first_difference: u64 = 0;
    let mut has_stack_depth: bool = false;
    let mut difference_types: Vec<DifferenceType> = vec![];

    // New bytes, something was added!
    if func1.content.len() != func2.content.len() {
        difference_types.push(DifferenceType::FunctionLength);
    }

    let instructions1: Vec<InstructionWrapper> =
        InstructionIter::new(func1.address, &func1.content, pointer_size).collect();
    let instructions2: Vec<InstructionWrapper> =
        InstructionIter::new(func2.address, &func2.content, pointer_size).collect();

    for (instr1, instr2) in std::iter::zip(&instructions1, &instructions2) {
        first_difference = instr1.get().ip() - func1.address;

        if instr1 != instr2 {
            difference_types.push(DifferenceType::DifferentInstruction);
            break;
        }

        // Opcode matches, let's check for stack depth
        // FIXME: Only handles 32-bit register
        // sub esp, <depth>
        if !has_stack_depth
            && instr1.get().mnemonic() == Mnemonic::Sub
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

            has_stack_depth = true;
        }
    }

    if !difference_types.is_empty() {
        // NOTE: Lcs panics on oob, wtf?
        let diffops =
            similar::capture_diff_slices(similar::Algorithm::Myers, &instructions1, &instructions2);

        CompareResult::Differs(CompareInfo {
            first_difference,
            difference_types,
            diffops,
            instructions: (instructions1, instructions2),
        })
    } else {
        CompareResult::Same()
    }
}
