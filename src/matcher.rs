use crate::program::{Function, Program};
use std::collections::HashMap;

pub enum MatchResult<'a> {
    Matched((&'a Function, &'a Function)),
    Unmatched,
    Finished,
}

pub struct FunctionMatcher<'a> {
    program1_functions: Vec<(&'a str, &'a Function)>,
    program2_functions: HashMap<&'a str, &'a Function>,

    program1_unmatched: Vec<(&'a str, &'a Function)>,
}

impl<'a> FunctionMatcher<'a> {
    pub fn new(program1: &'a Program, program2: &'a Program) -> Self {
        Self {
            program1_functions: program1
                .functions
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect(),
            program2_functions: program2
                .functions
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect(),

            program1_unmatched: vec![],
        }
    }

    pub fn next_match(&mut self) -> MatchResult<'a> {
        if let Some((func1_name, func1)) = self.program1_functions.pop() {
            if let Some(func2) = self.program2_functions.remove(&func1_name) {
                return MatchResult::Matched((func1, func2));
            }

            self.program1_unmatched.push((func1_name, func1));
            return MatchResult::Unmatched;
        }

        MatchResult::Finished
    }

    pub fn get_unmatched(self) -> (Vec<(&'a str, &'a Function)>, Vec<(&'a str, &'a Function)>) {
        let program2_unmatched = self.program2_functions.into_iter().collect();
        (self.program1_unmatched, program2_unmatched)
    }
}
