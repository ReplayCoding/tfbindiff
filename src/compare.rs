use iced_x86::{Instruction, Mnemonic, OpKind, Register};

pub struct Function {
    pub address: u64,
    pub content: Vec<u8>,
}

impl Function {
    pub fn new(address: u64, content: Vec<u8>) -> Function {
        Function { address, content }
    }
}

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
}

fn get_stack_depth_from_instruction(instr: &Instruction) -> i64 {
    match instr.op1_kind() {
        OpKind::Immediate8to32 => instr.immediate8to32().into(),
        OpKind::Immediate32 => instr.immediate32().into(),
        _ => todo!("stack depth: unhandled op1 type {:?}", instr.op1_kind()),
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

    let code1 = dump_code(func1.address, &func1.content, pointer_size);
    let code2 = dump_code(func2.address, &func2.content, pointer_size);

    let mut info_factory1 = iced_x86::InstructionInfoFactory::new();
    let mut info_factory2 = iced_x86::InstructionInfoFactory::new();
    for (instr1, instr2) in std::iter::zip(code1, code2) {
        first_difference = instr1.ip() - func1.address;

        let op_code1 = instr1.op_code();
        let op_code2 = instr2.op_code();

        // Opcode doesn't match
        if instr1.code() != instr2.code() {
            difference_types.push(DifferenceType::DifferentInstruction);
            break;
        }

        // Opcode matches, let's check for stack depth
        // FIXME: Only handles 32-bit register
        // sub esp, <depth>
        if !has_stack_depth
            && instr1.mnemonic() == Mnemonic::Sub
            && instr1.op0_kind() == OpKind::Register
            && instr1.op0_register() == Register::ESP
            && instr2.op0_kind() == OpKind::Register
            && instr2.op0_register() == Register::ESP
        {
            let stack_depth1: i64 = get_stack_depth_from_instruction(&instr1);
            let stack_depth2: i64 = get_stack_depth_from_instruction(&instr2);

            if stack_depth1 != stack_depth2 {
                difference_types.push(DifferenceType::StackDepth);
                // println!("{:08X}: {} {}", func1.address, stack_depth1, stack_depth2);
            }

            has_stack_depth = true;
        }

        // Operand count doesn't match
        if op_code1.op_count() != op_code2.op_count() {
            difference_types.push(DifferenceType::DifferentInstruction);
            break;
        }

        for i in 0..op_code1.op_count() {
            let op_kind1 = op_code1.op_kind(i);
            let op_kind2 = op_code2.op_kind(i);

            // Operand kind doesn't match
            if op_kind1 != op_kind2 {
                difference_types.push(DifferenceType::DifferentInstruction);
                break;
            }
        }

        let info1 = info_factory1.info(&instr1);
        let info2 = info_factory2.info(&instr2);

        if info1.used_registers() != info2.used_registers() {
            difference_types.push(DifferenceType::DifferentInstruction);
            break;
        }
    }

    if !difference_types.is_empty() {
        CompareResult::Differs(CompareInfo {
            first_difference,
            difference_types,
        })
    } else {
        CompareResult::Same()
    }
}