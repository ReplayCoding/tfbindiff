use iced_x86::{Decoder, DecoderOptions, Instruction, OpKind};
use std::hash::Hash;

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
        if (self.0.code() == other.0.code())
            && (self.0.op_code().op_kinds() == other.0.op_code().op_kinds())
        {
            for op_idx in 0..self.0.op_count() {
                if self.0.op_kind(op_idx) == OpKind::Register {
                    let reg1 = self.0.op_register(op_idx);
                    let reg2 = other.0.op_register(op_idx);

                    if reg1 != reg2 {
                        return false;
                    }
                };
            }
            return true;
        }

        false
    }
}

impl Hash for InstructionWrapper {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {
        // See: https://github.com/mitsuhiko/similar/issues/50, same for Ord
        todo!("implement Hash for instructions")
    }
}

impl Ord for InstructionWrapper {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        todo!("implement Ord for instructions")
    }
}

impl PartialOrd for InstructionWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct InstructionIter<'a> {
    decoder: Decoder<'a>,
}

impl<'a> InstructionIter<'a> {
    pub fn new(address: u64, code: &'a [u8], address_size: usize) -> Self {
        Self {
            decoder: Decoder::with_ip(
                (address_size * 8).try_into().unwrap(),
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
