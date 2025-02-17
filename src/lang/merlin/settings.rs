//! Parse settings string sent by any client.
//! 
//! The server will check for specific keys that may affect its operation.
//! The settings structure can then be used by the various modules.

use serde_json;
use crate::DYNERR;
use crate::lang::{update_json_bool,update_json_i64,update_json_f64,update_json_severity};
use lsp_types::DiagnosticSeverity;

#[derive(Clone)]
pub struct Flag {
    pub case_sensitive: Option<DiagnosticSeverity>,
    pub unclosed_folds: Option<DiagnosticSeverity>
    // TODO: major version: pub unused_macros: Option<DiagnosticSeverity>
    // TODO: major version: pub dup_mac_locs: Option<DiagnosticSeverity>
}
#[derive(Clone)]
pub struct Columns {
    pub c1: i64,
    pub c2: i64,
    pub c3: i64
}
#[derive(Clone)]
pub struct Linker {
    pub detect: f64
}
#[derive(Clone)]
pub struct Hovers {
    pub special_addresses: bool,
    pub mnemonics: bool,
    pub pseudo: bool
}
#[derive(Clone)]
pub struct Completions {
    pub lower_case: bool,
    pub ibas: bool,
    pub abas: bool
}
#[derive(Clone)]
pub struct Diagnostics {
    pub live: bool
}
#[derive(Clone)]
pub struct Disassembly {
    pub brk: bool
}
#[derive(Clone)]
pub struct Settings {
    pub version: super::MerlinVersion,
    pub flag: Flag,
    pub columns: Columns,
    pub linker: Linker,
    pub hovers: Hovers,
    pub completions: Completions,
    pub disassembly: Disassembly,
    pub diagnostics: Diagnostics
}

impl Settings {
    pub fn new() -> Self {
        Self {
            version: super::MerlinVersion::Merlin8,
            flag : Flag {
                case_sensitive: None,
                unclosed_folds: Some(DiagnosticSeverity::ERROR)
                // TODO: major version: unused_macros: Some(DiagnosticSeverity::HINT)
                // TODO: major version: dup_mac_locs: Some(DiagnosticSeverity::WARNING)
            },
            columns : Columns {
                c1: 9,
                c2: 6,
                c3: 11
            },
            linker: Linker {
                detect: 0.1
            },
            hovers : Hovers {
                special_addresses: true,
                mnemonics: true,
                pseudo: true
            },
            completions : Completions {
                lower_case: false,
                ibas: false,
                abas: true
            },
            disassembly : Disassembly {
                brk: false,
            },
            diagnostics: Diagnostics {
                live: true
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
                    "version" => {
                        match val.as_str() {
                            Some("Merlin 8") => ans.version = super::MerlinVersion::Merlin8,
                            Some("Merlin 16") => ans.version = super::MerlinVersion::Merlin16,
                            Some("Merlin 16+") => ans.version = super::MerlinVersion::Merlin16Plus,
                            Some("Merlin 32") => ans.version = super::MerlinVersion::Merlin32,
                            _ => ans.version = super::MerlinVersion::Merlin8
                        }
                    }
                    "flag" => {
                        update_json_severity(val,"caseSensitive",&mut ans.flag.case_sensitive);
                        update_json_severity(val,"unclosedFolds",&mut ans.flag.unclosed_folds);
                        // TODO: major version: update_json_severity(val,"unusedMacros",&mut ans.flag.unused_macros);
                        // TODO: major version: update_json_severity(val,"duplicateMacroLocals",&mut ans.flag.dup_mac_locs);
                    },
                    "columns" => {
                        update_json_i64(val,"c1",&mut ans.columns.c1);
                        update_json_i64(val,"c2",&mut ans.columns.c2);
                        update_json_i64(val,"c3",&mut ans.columns.c3);
                    },
                    "linker" => {
                        update_json_f64(val,"detect",&mut ans.linker.detect);
                    },
                    "hovers" => {
                        update_json_bool(val,"specialAddresses",&mut ans.hovers.special_addresses);
                        update_json_bool(val,"mnemonics",&mut ans.hovers.mnemonics);
                        update_json_bool(val,"pseudo",&mut ans.hovers.pseudo);
                    },
                    "completions" => {
                        update_json_bool(val,"lowerCase",&mut ans.completions.lower_case);
                        update_json_bool(val,"ibas",&mut ans.completions.ibas);
                        update_json_bool(val,"abas",&mut ans.completions.abas);
                    },
                    "disassembly" => {
                        update_json_bool(val, "brk", &mut ans.disassembly.brk);
                    },
                    "diagnostics" => {
                        update_json_bool(val, "live", &mut ans.diagnostics.live);
                    },
                    _ => {}
                }
            }
        }
    }
    Ok(ans)
}