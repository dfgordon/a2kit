//! Parse settings string sent by any client.
//! 
//! The server will check for specific keys that may affect its operation.
//! These are then used by the various modules to set their own flags.

use serde_json;
use crate::DYNERR;
use crate::lang::{update_json_bool,update_json_i64,update_json_vec,update_json_severity};
use lsp_types::DiagnosticSeverity;

#[derive(Clone)]
pub struct Flag {
    pub case_sensitive: Option<DiagnosticSeverity>,
    pub terminal_string: Option<DiagnosticSeverity>,
    pub collisions: Option<DiagnosticSeverity>,
    pub undeclared_arrays: Option<DiagnosticSeverity>,
    pub undefined_variables: Option<DiagnosticSeverity>,
    pub bad_references: Option<DiagnosticSeverity>,
    pub extended_call: Option<DiagnosticSeverity>
}
#[derive(Clone)]
pub struct Hovers {
    pub special_addresses: bool,
    pub keywords: bool
}
#[derive(Clone)]
pub struct Completions {
    pub lower_case: bool,
    pub negative_addresses: bool
}
#[derive(Clone)]
pub struct Detokenizer {
    pub escapes: Vec<i64>,
    pub max_lines: i64,
    pub max_line_length: i64
}
#[derive(Clone)]
pub struct Settings {
    pub flag: Flag,
    pub hovers: Hovers,
    pub completions: Completions,
    pub detokenizer: Detokenizer
}

impl Settings {
    pub fn new() -> Self {
        Self {
            flag : Flag {
                case_sensitive: None,
                terminal_string: Some(DiagnosticSeverity::WARNING),
                collisions: Some(DiagnosticSeverity::WARNING),
                undeclared_arrays: Some(DiagnosticSeverity::WARNING),
                undefined_variables: Some(DiagnosticSeverity::WARNING),
                bad_references: Some(DiagnosticSeverity::ERROR),
                extended_call: Some(DiagnosticSeverity::ERROR)
            },
            hovers : Hovers {
                special_addresses: true,
                keywords: true
            },
            completions : Completions {
                lower_case: true,
                negative_addresses: false
            },
            detokenizer : Detokenizer {
                escapes: vec![10,13],
                max_lines: 5000,
                max_line_length: 255
            }
        }
    }
}

pub fn parse(json: &str) -> Result<Settings,DYNERR> {
    let mut ans = Settings::new();
    if let Ok(root) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(obj) = root.as_object() {
            for (key,val) in obj {
                match key.as_str() {
                    "flag" => {
                        update_json_severity(val,"caseSensitive",&mut ans.flag.case_sensitive);
                        update_json_severity(val,"terminalString", &mut ans.flag.terminal_string);
                        update_json_severity(val,"collisions",&mut ans.flag.collisions);
                        update_json_severity(val,"undeclaredArrays",&mut ans.flag.undeclared_arrays);
                        update_json_severity(val,"undefinedVariables",&mut ans.flag.undefined_variables);
                        update_json_severity(val,"badReferences",&mut ans.flag.bad_references);
                        update_json_severity(val,"extendedCall",&mut ans.flag.extended_call);
                    },
                    "hovers" => {
                        update_json_bool(val,"specialAddresses",&mut ans.hovers.special_addresses);
                        update_json_bool(val,"keywords",&mut ans.hovers.keywords);
                    },
                    "completions" => {
                        update_json_bool(val,"lowerCaseCompletions",&mut ans.completions.lower_case);
                        update_json_bool(val,"negativeAddresses",&mut ans.completions.negative_addresses);
                    },
                    "detokenizer" => {
                        update_json_i64(val, "maxLineLength", &mut ans.detokenizer.max_line_length);
                        update_json_i64(val,"maxLines",&mut ans.detokenizer.max_lines);
                        update_json_vec(val,"escapes",&mut ans.detokenizer.escapes);
                    },
                    _ => {}
                }
            }
        }
    }
    Ok(ans)
}