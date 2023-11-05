mod compare;
mod eh_frame;
mod instruction_wrapper;
mod matcher;
mod output;
mod program;

use compare::compare_programs;
use output::print_changes;
use program::Program;

fn main() {
    let args: Vec<_> = std::env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let (program1, program2) = rayon::join(
        || Box::new(Program::load_path(&args[1])),
        || Box::new(Program::load_path(&args[2])),
    );

    let changes = compare_programs(&program1, &program2);
    print_changes(program1, program2, &changes);
}
