mod compare;
mod eh_frame;
mod instruction_wrapper;
mod matcher;
mod output;
mod program;

use crate::compare::{compare_functions, CompareResult};
use crate::matcher::FunctionMatcher;
use crate::output::demangle_symbol;
use crate::output::FunctionChange;
use crate::program::Program;
use std::env;

use rayon::prelude::*;

fn get_changes(program1: &Program, program2: &Program) -> Vec<FunctionChange> {
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
                CompareResult::Differs(compare_info) => {
                    let name: String = demangle_symbol(name).unwrap_or(name.to_string());

                    Some(FunctionChange::new(
                        compare_info,
                        name,
                        func1.address,
                        func2.address,
                    ))
                }
                CompareResult::Same() => None,
            }
        })
        .collect();

    changes.par_sort_by(|a, b| a.address1.cmp(&b.address1));

    changes
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <primary> <secondary>", args[0]);
        return;
    }

    let (program1, program2) = rayon::join(
        || Box::new(Program::load_path(&args[1])),
        || Box::new(Program::load_path(&args[2])),
    );

    let changes = get_changes(&program1, &program2);
    crate::output::print_changes(program1, program2, &changes);
}
