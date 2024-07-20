use tree_sitter;
use a2_memory_map as a2memory;
use crate::lang::node_radix;
use std::collections::HashMap;
use std::format;
use num_traits::abs;

pub struct AddressHovers {
    amap: HashMap<u16,String>
}

const OFFSET_NAMES: [(&str,[&str;6]);4] = [
    ("word",["low byte","high byte","","","",""]),
    ("vector",["opcode","low addr","high addr","","",""]),
    ("float",["1","2","3","4","5",""]),
    ("unpacked float",["1","2","3","4","5","6"])
];

fn create_hover(addr_base: u16,offset: u16,info: &a2memory::AddressInfo) -> String {
    let offset_names = HashMap::from(OFFSET_NAMES);
    let addr_unsigned = addr_base as i32 + offset as i32;
    let addr_signed = addr_unsigned - 1 - u16::MAX as i32;
    let mut ans = String::new();
    if let Some(label) = &info.label {
        ans += &format!("`{}`\n\n",label);
    }
    let mut addr_type = info.typ.clone();
    if let Some(subtype) = offset_names.get(info.typ.as_str()) {
        addr_type += ", ";
        addr_type += subtype[offset as usize];
    }
    if addr_unsigned >= (1<<15) {
        ans += &format!("Special address: **{}** ({} | {} | ${:X})\n\n",addr_type,addr_unsigned,addr_signed,addr_unsigned);
    } else {
        ans += &format!("Special address: **{}** ({} | ${:X})\n\n",addr_type,addr_unsigned,addr_unsigned);
    }
    ans += &info.desc;
    ans += "\n\n";
    if let Some(ctx) = &info.ctx {
        ans += &format!("Context limitation: {}\n\n",ctx);
    }
    if let Some(note) = &info.note {
        ans += &format!("Note: {}\n\n",note);
    }
    ans
}

impl AddressHovers {
    pub fn new() -> Self {
        let mut hov = Self {
            amap: HashMap::new()
        };
        let main_map = a2memory::MemoryMap::new();
        let raw_map = main_map.get_all();
        for (addr,info) in raw_map {
            let bytes = match info.typ.as_str() {
                "word" => 2,
                "vector" => 3,
                "float" => 5,
                "unpacked float" => 6,
                _ => 1
            };
            for offset in 0..bytes {
                let hov_str = create_hover(*addr, offset, info);
                hov.amap.insert(*addr,hov_str);
            }
        }
        hov
    }
    /// OK to pass out of range number; negative address will be translated if possible
    pub fn get(&self,addr: i64) -> Option<String> {
        if abs(addr) > u16::MAX as i64 {
            return None;
        }
        let addr16 = match addr >= 0 {
            true => addr as u16,
            false => (addr + 1 + u16::MAX as i64) as u16
        };
        self.amap.get(&addr16).cloned()
    }
    pub fn get_from_node(&self,curs: &tree_sitter::TreeCursor,line: &str) -> Option<String> {
        if curs.node().kind() == "num" {
            if let Some(num) = node_radix::<i64>(&curs.node(),line,"$","%") {
                return self.get(num);
            }
        }
        None
    }
}