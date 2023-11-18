mod compare;
mod eh_frame;
mod gui;
mod instruction_wrapper;
mod matcher;
mod program;
mod split_diff;
mod util;

use crate::compare::compare_programs;
use crate::program::Program;
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let (program1, program2) = rayon::join(
        || Box::new(Program::load(Path::new(&args[1]))),
        || Box::new(Program::load(Path::new(&args[2]))),
    );

    let changes = compare_programs(&program1, &program2);

    gui::run(Box::leak(program1), Box::leak(program2), &changes);
}
