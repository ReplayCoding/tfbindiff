use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex_lite::Regex;

use crate::program::{Function, Program};

static STATIC_INITIALIZER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^_?_GLOBAL__sub_I_(.*)\.stdout\.rel_tf_osx_builder\..*\.ii$").unwrap()
});

pub struct FunctionMatcher<'a> {
    program: &'a Program,
    static_init_map: HashMap<String, &'a str>,
}

impl<'a> FunctionMatcher<'a> {
    pub fn new(program: &'a Program) -> Self {
        let static_init_map = Self::build_static_init_map(&program.functions);
        Self {
            program,
            static_init_map,
        }
    }

    pub fn match_name(&self, name: &str) -> Option<&'a Function> {
        if let Some(func2) = self.program.functions.get(name) {
            Some(func2)
        } else if let Some(captures) = STATIC_INITIALIZER_REGEX.captures(name) {
            let extracted_filename = &captures[1];
            let name2 = self.static_init_map.get(extracted_filename)?;

            self.program.functions.get(*name2)
        } else {
            None
        }
    }

    fn build_static_init_map(functions: &HashMap<String, Function>) -> HashMap<String, &str> {
        let mut static_initializers_to_note: HashMap<String, &str> = Default::default();

        let mut static_initializer_blocklist: HashSet<String> = Default::default();
        for name in functions.keys() {
            if let Some(captures) = STATIC_INITIALIZER_REGEX.captures(name) {
                let extracted_filename = captures.get(1).unwrap().as_str();
                if static_initializer_blocklist.contains(extracted_filename) {
                    continue;
                }

                if !static_initializers_to_note.contains_key(extracted_filename) {
                    static_initializers_to_note.insert(extracted_filename.to_string(), name);
                } else {
                    static_initializers_to_note.remove(extracted_filename);
                    static_initializer_blocklist.insert(extracted_filename.to_string());
                }
            }
        }

        static_initializers_to_note
    }
}
