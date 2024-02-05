use crate::instruction_wrapper::{InstructionIter, InstructionWrapper};
use crate::matcher::{FunctionMatcher, MatchResult};
use crate::program::{Function, Program};
use iced_x86::{Instruction, Mnemonic, OpKind, Register};
use itertools::Itertools;

enum CompareResult {
    Same(),
    Differs(CompareInfo),
}

#[derive(Clone)]
struct CompareInfo {
    instructions: (Vec<InstructionWrapper>, Vec<InstructionWrapper>),
}

fn get_stack_depth_from_instruction(instr: &Instruction) -> i64 {
    match instr.op1_kind() {
        OpKind::Immediate8to32 => instr.immediate8to32().into(),
        OpKind::Immediate32 => instr.immediate32().into(),
        _ => todo!("stack depth: unhandled op1 type {:?}", instr.op1_kind()),
    }
}

fn create_instruction_iter<'a>(program: &'a Program, func: &Function) -> InstructionIter<'a> {
    let func_content = program.get_data_for_function(func).unwrap();
    InstructionIter::new(func.address(), func_content, program.pointer_size)
}

fn compare_functions(
    program1: &Program,
    program2: &Program,
    func1: &Function,
    func2: &Function,
) -> CompareResult {
    let mut has_difference = false;

    let instructions1 = create_instruction_iter(program1, func1);
    let instructions2 = create_instruction_iter(program2, func2);

    for zipped in instructions1.zip_longest(instructions2) {
        match zipped {
            itertools::EitherOrBoth::Both(instr1, instr2) => {
                if instr1 != instr2 {
                    has_difference = true;
                    break;
                }

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
            itertools::EitherOrBoth::Left(_) | itertools::EitherOrBoth::Right(_) => {
                has_difference = true;
                break;
            }
        }
    }

    if has_difference {
        let instructions1 = create_instruction_iter(program1, func1).collect();
        let instructions2 = create_instruction_iter(program2, func2).collect();
        CompareResult::Differs(CompareInfo {
            instructions: (instructions1, instructions2),
        })
    } else {
        CompareResult::Same()
    }
}

#[derive(Clone)]
pub struct FunctionChange {
    info: CompareInfo,
    name: String,
    address1: u64,
    address2: u64,
}

impl FunctionChange {
    fn new(info: CompareInfo, name: String, address1: u64, address2: u64) -> Self {
        Self {
            info,
            name,
            address1,
            address2,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn instructions(&self) -> (&[InstructionWrapper], &[InstructionWrapper]) {
        (&self.info.instructions.0, &self.info.instructions.1)
    }

    pub fn address1(&self) -> u64 {
        self.address1
    }

    pub fn address2(&self) -> u64 {
        self.address2
    }
}

pub fn compare_programs(program1: &Program, program2: &Program) -> Vec<FunctionChange> {
    assert!(
        program1.pointer_size == program2.pointer_size,
        "pointer sizes don't match"
    );

    let mut matcher = FunctionMatcher::new(program1, program2);

    let mut changes: Vec<FunctionChange> = vec![];
    loop {
        match matcher.next_match() {
            MatchResult::Matched((func1, func2)) => {
                if let CompareResult::Differs(compare_info) =
                    compare_functions(program1, program2, func1, func2)
                {
                    let name = program1.symbol_map.get(&func1.address()).unwrap();
                    changes.push(FunctionChange::new(
                        compare_info,
                        name.to_string(),
                        func1.address(),
                        func2.address(),
                    ));
                }
            }
            MatchResult::Unmatched => (),
            MatchResult::Finished => break,
        }
    }

    changes.sort_by(|a, b| a.address1.cmp(&b.address1));

    // TODO: return this for usage in the GUI
    let (_program1_unmatched, _program2_unmatched) = matcher.get_unmatched();

    changes
}
