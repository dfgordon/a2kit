use std::collections::{HashSet,HashMap};
use super::super::{PseudoOperation,MerlinVersion};
use crate::lang::{update_json_string_opt, update_json_string, update_json_vec_str};

const RCH: &str = "unreachable was reached";
const JSON_STR: &str = include_str!("pseudo_ops.json");

/// Try to update a Merlin version list from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_vers(maybe_obj: &serde_json::Value, key: &str, curr: &mut Vec<MerlinVersion>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            let mut ans: Vec<MerlinVersion> = Vec::new();
            if let Some(a) = x.as_array() {
                for v in a {
                    match v.as_str() {
                        Some("v8") => ans.push(MerlinVersion::Merlin8),
                        Some("v16") => ans.push(MerlinVersion::Merlin16),
                        Some("v16+") => ans.push(MerlinVersion::Merlin16Plus),
                        Some("v32") => ans.push(MerlinVersion::Merlin32),
                        _ => return
                    }
                }
                *curr = ans;
            }
        }
    }
}

/// This can panic if something is wrong with the JSON.
/// But the JSON practically never changes.
fn build_pseudo_ops() -> HashMap<String,PseudoOperation> {
    let mut ans = HashMap::new();
    let root_val = serde_json::from_str::<serde_json::Value>(JSON_STR).unwrap();
    if let Some(root) = root_val.as_object() {
        for (mnemonic,info) in root {
            let mut psop = PseudoOperation::new();
            update_json_vec_str(info, "alt", &mut psop.alt);
            update_json_string(info, "brief", &mut psop.brief);
            update_json_string(info,"category",&mut psop.category);
            update_json_string_opt(info,"caveat",&mut psop.caveat);
            update_json_string(info, "desc", &mut psop.desc);
            update_json_vec_str(info, "eg", &mut psop.eg);
            update_json_vec_str(info,"enum",&mut psop.choices);
            update_json_vers(info,"version",&mut psop.version);

            let mut v8x = None;
            let mut v16x = None;
            update_json_string_opt(info,"v8x",&mut v8x);
            update_json_string_opt(info,"v16x",&mut v16x);
            if let Some(re) = v8x {
                let v8regex = "(?i)".to_string() + &re[1..re.len()-1];
                psop.v8x = Some(regex::Regex::new(&v8regex).expect(RCH));
            }
            if let Some(re) = v16x {
                let v16regex = "(?i)".to_string() + &re[1..re.len()-1];
                psop.v16x = Some(regex::Regex::new(&v16regex).expect(RCH));
            }
            ans.insert(mnemonic.to_string(),psop);
        }
    }
    ans
}

impl PseudoOperation {
    pub fn new() -> Self {
        Self {
            alt: Vec::new(),
            brief: "".to_string(),
            category: "".to_string(),
            caveat: None,
            desc: "".to_string(),
            eg: Vec::new(),
            choices: Vec::new(),
            v8x: None,
            v16x: None,
            version: Vec::new()
        }
    }
}

pub struct PseudoOperationHandbook {
    ops_8: HashSet<String>,
    ops_16: HashSet<String>,
    ops_16p: HashSet<String>,
    ops_32: HashSet<String>,
    alternates: HashMap<String,String>,
    ops: HashMap<String,PseudoOperation>
}

impl PseudoOperationHandbook {
    pub fn new() -> Self {
        let ops = build_pseudo_ops();
        let mut ops_8 = HashSet::new();
        let mut ops_16 = HashSet::new();
        let mut ops_16p = HashSet::new();
        let mut ops_32 = HashSet::new();
        let mut alternates = HashMap::new();
        for (key,op) in &ops {
            let mut mnemonics = vec![key];
            for alt in &op.alt {
                alternates.insert(alt.to_owned(),key.to_owned());
                mnemonics.push(alt);
            }
            for mnemonic in mnemonics {
                if op.version.contains(&MerlinVersion::Merlin8) {
                    ops_8.insert(mnemonic.to_owned());
                }
                if op.version.contains(&MerlinVersion::Merlin16) {
                    ops_16.insert(mnemonic.to_owned());
                }
                if op.version.contains(&MerlinVersion::Merlin16Plus) {
                    ops_16p.insert(mnemonic.to_owned());
                }
                if op.version.contains(&MerlinVersion::Merlin32) {
                    ops_32.insert(mnemonic.to_owned());
                }
            }
        }
        Self {
            ops_8,
            ops_16,
            ops_16p,
            ops_32,
            alternates,
            ops
        }
    }
    fn normalize(mnemonic: &str) -> String {
        match mnemonic.len() > 3 {
            true => {
                let lower = mnemonic.to_lowercase();
                if lower.starts_with("adrl") || lower.starts_with("dend") || lower.starts_with("else") || lower.starts_with("strl") {
                    return lower[0..4].to_string();
                }
                if lower.starts_with("lstdo") {
                    return lower[0..5].to_string();
                }
                lower[0..3].to_string()
            },
            false => mnemonic.to_lowercase()
        }
    }
    /// Does this mnemonic or alternate exist for the given version.
    /// To test for any, set version to Merlin16Plus.
    /// Case insensitive, otherwise requires exact match like Merlin 32.
    pub fn strong_match(&self, mnemonic: &str, version: &MerlinVersion) -> bool {
        match version {
            MerlinVersion::Merlin8 => self.ops_8.contains(&mnemonic.to_lowercase()),
            MerlinVersion::Merlin16 => self.ops_16.contains(&mnemonic.to_lowercase()),
            MerlinVersion::Merlin16Plus => self.ops_16p.contains(&mnemonic.to_lowercase()),
            MerlinVersion::Merlin32 => self.ops_32.contains(&mnemonic.to_lowercase()),
        }
    }
    /// Same as strong_match, except checks at most the first n characters of `mnemonic`, where n is the "normal" length of the mnemonic.
    pub fn weak_match(&self,mnemonic: &str,version: &MerlinVersion) -> bool {
        match version {
            MerlinVersion::Merlin8 => self.ops_8.contains(&Self::normalize(&mnemonic)),
            MerlinVersion::Merlin16 => self.ops_16.contains(&Self::normalize(&mnemonic)),
            MerlinVersion::Merlin16Plus => self.ops_16p.contains(&Self::normalize(&mnemonic)),
            MerlinVersion::Merlin32 => self.ops_32.contains(&Self::normalize(&mnemonic)),
        }
    }
    /// Find the operation corresponding to a mnemonic or any of its alternates.
    /// Case insensitive, accepts weak matches, search all versions.
    pub fn get(&self,mnemonic_or_alt: &str) -> Option<PseudoOperation> {
        let norm = Self::normalize(&mnemonic_or_alt);
        match self.alternates.get(&norm) {
            Some(mnemonic) => self.ops.get(mnemonic).cloned(),
            None => self.ops.get(&norm).cloned()
        }
    }
    pub fn completion(&self,version: &MerlinVersion) -> Vec<String> {
        match version {
            MerlinVersion::Merlin8 => self.ops_8.iter().map(|s| s.to_string()).collect(),
            MerlinVersion::Merlin16 => self.ops_16.iter().map(|s| s.to_string()).collect(),
            MerlinVersion::Merlin16Plus => self.ops_16p.iter().map(|s| s.to_string()).collect(),
            MerlinVersion::Merlin32 => self.ops_32.iter().map(|s| s.to_string()).collect(),
        }
    }
}

