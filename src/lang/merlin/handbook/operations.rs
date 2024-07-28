use std::collections::{HashSet,HashMap};
use super::super::{Operation,MachineOperation,ProcessorType,AddressMode};
use crate::lang::{update_json_i64, update_json_string, update_json_vec_str};

const JSON_STR: &str = include_str!("opcodes.json");

/// Map from a machine addressing mode to the reduced modes used by the parser.
const UNPARSING_MAP: [(&'static str,&'static str);26] =  [
    ("imm","imm"),
    ("imm_zp","data"),
    ("imm_abs","data"),
    ("zp","addr"),
    ("abs","addr"),
    ("absl","addr"),
    ("rel","addr"),
    ("rell","addr"),
    ("(zp,x)","iaddr_ix"),
    ("(abs,x)","iaddr_ix"),
    ("(zp),y","iaddr_y"),
    ("zp,x","addr_x"),
    ("abs,x","addr_x"),
    ("absl,x","addr_x"),
    ("zp,y","addr_y"),
    ("abs,y","addr_y"),
    ("(abs)","iaddr"),
    ("(zp)","iaddr"),
    ("[d]","daddr"),
    ("[d],y","daddr_y"),
    ("d,s","addr_s"),
    ("(d,s),y","iaddr_is_y"),
    ("xyc","xyc"),
	("impl", "impl"),
	("accum", "accum"),
	("s", "s")
];

/// Map used to find a machine addressing mode based on a reduced
/// addressing mode used by the parser, and an operand length.
/// Relative addressing has to be handled separately.
const PARSING_MAP: [(&'static str,&'static str);25] =  [
    ("imm 1","imm"), // selected based on MX
    ("imm 2","imm"), // selected based on MX
    ("data 1","imm_zp"),
    ("data 2","imm_abs"),
    ("addr 1","zp"),
    ("addr 2","abs"),
    ("addr 3","absl"),
    ("iaddr_ix 1","(zp,x)"),
    ("iaddr_ix 2","(abs,x)"),
    ("iaddr_y 1","(zp),y"),
    ("addr_x 1","zp,x"),
    ("addr_x 2","abs,x"),
    ("addr_x 3","absl,x"),
    ("addr_y 1","zp,y"),
    ("addr_y 2","abs,y"),
    ("iaddr 2","(abs)"),
    ("iaddr 1","(zp)"),
    ("daddr 1","[d]"),
    ("daddr_y 1","[d],y"),
    ("addr_s 1","d,s"),
    ("iaddr_is_y 1","(d,s),y"),
    ("xyc 11","xyc"),
	("impl 0", ""),
	("accum 0", ""),
	("s 0", "")
];

/// Map from machine addressing modes to a snippet where we can simply search for a number,
/// and the number is the length of the binary data.  Furthermore,
/// the source is formed by substituting the value of the data for the number
/// (but n.b. for relative branches the display number is not the actual one).
/// For immediate mode MX!=11 has to be handled separately.
const DASM_MAP: [(&str,&str);26] = [
    ("impl", ""),
    ("accum", ""),
    ("s", ""),
    ("imm", "#1"), // assumes MX=11
    ("imm_zp", "1"),
    ("imm_abs", "2"),
    ("abs", "2"),
    ("zp", "1"),
    ("rel", "1"),
    ("rell", "2"),
    ("absl", "3"),
    ("(zp,x)", "(1,x)"),
    ("(abs,x)", "(2,x)"),
    ("(zp),y", "(1),y"),
    ("zp,x", "1,x"),
    ("abs,x", "2,x"),
    ("absl,x", "3,x"),
    ("zp,y", "1,y"),
    ("abs,y", "2,y"),
    ("(abs)", "(2)"),
    ("(zp)", "(1)"),
    ("[d]", "[1]"),
    ("[d],y", "[1],y"),
    ("d,s", "1,s"),
    ("(d,s),y", "(1,s),y"),
    ("xyc", "11")
];

/// instructions that are affected by the M bit, values are raw lower case mnemonics
const M_STATUS: [&'static str;8] = ["adc","and","bit","cmp","eor","lda","ora","sbc"];
/// instructions that are affected by the X bit, values are raw lower case mnemonics
const X_STATUS: [&'static str;4] = ["cpx","cpy","ldx","ldy"];

/// Try to update a processor list from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
fn update_json_proc(maybe_obj: &serde_json::Value, key: &str, curr: &mut Vec<ProcessorType>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            let mut ans: Vec<ProcessorType> = Vec::new();
            if let Some(a) = x.as_array() {
                for v in a {
                    match v.as_str() {
                        Some("6502") => ans.push(ProcessorType::_6502),
                        Some("65c02") => ans.push(ProcessorType::_65c02),
                        Some("65802") => ans.push(ProcessorType::_65802),
                        Some("65c816") => ans.push(ProcessorType::_65c816),
                        _ => return
                    }
                }
                *curr = ans;
            }
        }
    }
}

/// Try to update an address mode list from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
fn update_json_addr_mode(maybe_obj: &serde_json::Value, key: &str, m: bool, x: bool, curr: &mut Vec<AddressMode>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(mode_list) = outer.get(key) {
            let mut ans: Vec<AddressMode> = Vec::new();
            if let Some(a) = mode_list.as_array() {
                for v in a {
                    let mut mode = AddressMode::new();
                    update_json_string(v, "addr_mnemonic", &mut mode.mnemonic);
                    update_json_i64(v, "code", &mut mode.code);
                    update_json_i64(v, "cycles", &mut mode.cycles);
                    update_json_proc(v,"processors",&mut mode.processors);
                    mode.m_sensitive = m && mode.mnemonic=="imm";
                    mode.x_sensitive = x && mode.mnemonic=="imm";
                    ans.push(mode);
                }
                *curr = ans;
            }
        }
    }
}

fn setup_modifiers(mnemonic: &str, op: &mut Operation) {
    let mut has_zp = false;
    let mut has_abs = false;
    let mut has_absl = false;
    for mode in &op.modes {
        if mode.mnemonic=="zp" {
            has_zp = true;
        }
        if mode.mnemonic=="abs" {
            has_abs = true;
        }
        if mode.mnemonic=="absl" {
            has_absl = true;
        }
    }
    if mnemonic=="jmp" || mnemonic=="jsr" {
        op.absl_prefixable = true;
    } else {
        if has_zp && has_abs {
            op.abs_suffixable = true;
        }
        if has_abs && has_absl {
            op.absl_suffixable = true;
        }
    }
}

/// This can panic if something is wrong with the JSON.
/// But the JSON practically never changes.
fn build_operations() -> HashMap<String,Operation> {
    let mut ans = HashMap::new();
    let root_val = serde_json::from_str::<serde_json::Value>(JSON_STR).unwrap();
    if let Some(root) = root_val.as_object() {
        for (mnemonic,info) in root {
            let m = M_STATUS.contains(&mnemonic.as_str());
            let x = X_STATUS.contains(&mnemonic.as_str());
            let mut op = Operation::new();
            update_json_vec_str(info, "alt", &mut op.alt);
            update_json_string(info, "brief", &mut op.brief);
            update_json_string(info, "desc", &mut op.desc);
            update_json_addr_mode(info, "modes", m, x, &mut op.modes);
            update_json_proc(info, "processors", &mut op.processors);
            update_json_string(info, "status", &mut op.status);
            setup_modifiers(&mnemonic,&mut op);
            op.x_sensitive = x;
            op.m_sensitive = m;
            ans.insert(mnemonic.to_string(),op);
        }
    }
    ans
}

pub struct OperationHandbook {
    ops_6502: HashSet<String>,
    ops_65c02: HashSet<String>,
    ops_65802: HashSet<String>,
    ops_65c816: HashSet<String>,
    alternates: HashMap<String,String>,
    ops: HashMap<String,Operation>
}

impl AddressMode {
    pub fn new() -> Self {
        Self {
            mnemonic: "".to_string(),
            code: 0,
            cycles: 0,
            processors: Vec::new(),
            m_sensitive: false,
            x_sensitive: false
        }
    }
    /// Get the reduced mode used in parsing
    pub fn get_reduced_mode(&self) -> String {
        let map = HashMap::from(UNPARSING_MAP);
        map.get(&self.mnemonic.as_str()).unwrap().to_string()
    }
}

impl Operation {
    pub fn new() -> Self {
        Self {
            alt: Vec::new(),
            brief: "".to_string(),
            desc: "".to_string(),
            modes: Vec::new(),
            processors: Vec::new(),
            status: "".to_string(),
            m_sensitive: false,
            x_sensitive: false,
            abs_suffixable: false,
            absl_suffixable: false,
            absl_prefixable: false
        }
    }
    /// Use parsing information to get an exact address mode.
    /// * parsing_mode - the string slice representing the tree node's kind (e.g. imm, addr, ...)
    /// * byte_count - the number of bytes required to represent the expression, accounting for prefix modifiers
    pub fn get_address_mode(&self,parsing_mode: &str,byte_count: usize) -> Result<AddressMode,crate::DYNERR> {
        let map = HashMap::from(PARSING_MAP);
        let key = format!("{} {}",parsing_mode,byte_count);
        if let Some(target_mode) = map.get(&key.as_str()) {
            log::trace!("given {} look for {}",&key,target_mode);
            for mode in &self.modes {
                log::trace!("found {}",&mode.mnemonic);
                if &mode.mnemonic == *target_mode {
                    return Ok(mode.clone());
                }
            }
        }
        if parsing_mode=="addr" {
            log::trace!("try a relative address");
            for mode in &self.modes {
                if &mode.mnemonic == "rel" {
                    log::trace!("found rel");
                    return Ok(mode.clone());
                }
                if &mode.mnemonic == "rell" {
                    log::trace!("found rell");
                    return Ok(mode.clone());
                }
            }
        }
        Err(Box::new(crate::lang::Error::Syntax))
    }
}

impl OperationHandbook {
    pub fn new() -> Self {
        let ops = build_operations();
        let mut ops_6502 = HashSet::new();
        let mut ops_65c02 = HashSet::new();
        let mut ops_65802 = HashSet::new();
        let mut ops_65c816 = HashSet::new();
        let mut alternates = HashMap::new();
        for (key,op) in &ops {
            let mut mnemonics = vec![key];
            for alt in &op.alt {
                alternates.insert(alt.to_owned(),key.to_owned());
                mnemonics.push(alt);
            }
            for mnemonic in mnemonics {
                if op.processors.contains(&ProcessorType::_6502) {
                    ops_6502.insert(mnemonic.to_owned());
                }
                if op.processors.contains(&ProcessorType::_65c02) {
                    ops_65c02.insert(mnemonic.to_owned());
                }
                if op.processors.contains(&ProcessorType::_65c816) {
                    ops_65802.insert(mnemonic.to_owned());
                    ops_65c816.insert(mnemonic.to_owned());
                }
            }
        }
        Self {
            ops_6502,
            ops_65c02,
            ops_65802,
            ops_65c816,
            alternates,
            ops
        }
    }
    fn normalize(mnemonic: &str) -> String {
        match mnemonic.len() > 3 {
            true => mnemonic[0..3].to_lowercase(),
            false => mnemonic.to_lowercase()
        }
    }
    /// Does this mnemonic or alternate exist for the given processor.
    /// To test for any, the 65c816 can be specified.
    /// Case insensitive, otherwise requires exact match like Merlin 32.
    pub fn strong_match(&self,mnemonic: &str,proc: &ProcessorType) -> bool {
        match proc {
            ProcessorType::_6502 => self.ops_6502.contains(&mnemonic.to_lowercase()),
            ProcessorType::_65c02 => self.ops_65c02.contains(&mnemonic.to_lowercase()),
            ProcessorType::_65802 => self.ops_65802.contains(&mnemonic.to_lowercase()),
            ProcessorType::_65c816 => self.ops_65c816.contains(&mnemonic.to_lowercase())
        }
    }
    /// Match with the given processor using at most the first 3 characters of `mnemonic`.
    pub fn weak_match(&self,mnemonic: &str,proc: &ProcessorType) -> bool {
        match proc {
            ProcessorType::_6502 => self.ops_6502.contains(&Self::normalize(mnemonic)),
            ProcessorType::_65c02 => self.ops_65c02.contains(&Self::normalize(mnemonic)),
            ProcessorType::_65802 => self.ops_65802.contains(&Self::normalize(mnemonic)),
            ProcessorType::_65c816 => self.ops_65c816.contains(&Self::normalize(mnemonic))
        }
    }
    /// Find the operation corresponding to a mnemonic or any of its alternates.
    /// Case insensitive, accepts weak matches, search all processors.
    pub fn get(&self,mnemonic_or_alt: &str) -> Option<Operation> {
        let norm = Self::normalize(&mnemonic_or_alt);
        match self.alternates.get(&norm) {
            Some(mnemonic) => self.ops.get(mnemonic).cloned(),
            None => self.ops.get(&norm).cloned()
        }
    }
    pub fn completion(&self,proc: &ProcessorType) -> Vec<String> {
        match proc {
            ProcessorType::_6502 => self.ops_6502.iter().map(|s| s.to_string()).collect(),
            ProcessorType::_65c02 => self.ops_65c02.iter().map(|s| s.to_string()).collect(),
            ProcessorType::_65802 => self.ops_65802.iter().map(|s| s.to_string()).collect(),
            ProcessorType::_65c816 => self.ops_65c816.iter().map(|s| s.to_string()).collect(),
        }
    }
    /// The handbook may have overlapping opcodes, e.g., JMP >ADDR and JML ADDR are the same operation.
    /// This is only an issue when we are disassembling.
    /// In that case we need a way to consistently choose one or the other.
    /// Altering the choices made here has the potential to break the disassembler.
    fn use_proposed_op(prior: &MachineOperation, proposed: &MachineOperation) -> bool {
        if prior.mnemonic=="jmp" && proposed.mnemonic=="jml" {
            return true;
        }
        if prior.mnemonic=="jsr" && proposed.mnemonic=="jsl" {
            return true;
        }
        false
    }
    /// Map opcode to machine operation for disassembly purposes.
    /// On creation, `MachineOperation::operand_snippet` assumes MX=11.
    /// The disassembler will mutate it as necessary.
    pub fn create_dasm_map(&self) -> HashMap<u8,MachineOperation> {
        let mut ans = HashMap::new();
        let dasm_map: HashMap<&str,&str> = HashMap::from(DASM_MAP);
        for (key,book_op) in &self.ops {
            for mode in &book_op.modes {
                let op = MachineOperation {
                    mnemonic: key.to_string(),
                    operand_snippet: dasm_map.get(&mode.mnemonic.as_str()).unwrap().to_string(),
                    processors: mode.processors.clone(),
                    relative: mode.mnemonic.starts_with("rel"),
                    m_sensitive: mode.m_sensitive,
                    x_sensitive: mode.x_sensitive,
                    abs_suffixable: book_op.abs_suffixable,
                    absl_suffixable: book_op.absl_suffixable,
                    absl_prefixable: book_op.absl_prefixable
                };
                if let Some(prior) = ans.get(&(mode.code as u8)) {
                    if Self::use_proposed_op(prior, &op) {
                        ans.insert(mode.code as u8,op);
                    }
                } else {
                    ans.insert(mode.code as u8,op);
                }
            }
        }
        ans
    }
}
