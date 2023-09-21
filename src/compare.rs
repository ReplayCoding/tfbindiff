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
}

pub struct CompareInfo {
    pub first_difference: u64,
    pub difference_type: DifferenceType,
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
    let mut difference_type: Option<DifferenceType> = None;

    // New bytes, something was added!
    if func1.content.len() != func2.content.len() {
        difference_type = Some(DifferenceType::FunctionLength);
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
            difference_type = Some(DifferenceType::DifferentInstruction);
            break;
        }

        // Operand count doesn't match
        if op_code1.op_count() != op_code2.op_count() {
            difference_type = Some(DifferenceType::DifferentInstruction);
            break;
        }

        for i in 0..op_code1.op_count() {
            let op_kind1 = op_code1.op_kind(i);
            let op_kind2 = op_code2.op_kind(i);

            // Operand kind doesn't match
            if op_kind1 != op_kind2 {
                difference_type = Some(DifferenceType::DifferentInstruction);
                break;
            }
        }

        let info1 = info_factory1.info(&instr1);
        let info2 = info_factory2.info(&instr2);

        if info1.used_registers() != info2.used_registers() {
            difference_type = Some(DifferenceType::DifferentInstruction);
            break;
        }
    }

    if let Some(difference_type) = difference_type {
        CompareResult::Differs(CompareInfo {
            first_difference,
            difference_type,
        })
    } else {
        CompareResult::Same()
    }
}
