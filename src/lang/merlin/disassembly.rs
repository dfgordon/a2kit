//! Module to transform object code and data to Merlin source files
//! 
//! This is not intended to be entirely automatic, rather it is meant to be
//! part of a language server, wherein live human intervention is possible.
//! However, it can also be used from the command line for simple disassemblies.

use std::sync::Arc;
use std::collections::{HashSet,HashMap};
use hex::ToHex;
use crate::lang;
use crate::lang::merlin::{ProcessorType,settings::Settings,MachineOperation,Symbols};
use crate::lang::merlin::handbook::operations::OperationHandbook;
use super::formatter;
use crate::DYNERR;

pub enum DasmRange {
    All,
    LastBloadDos33,
    LastBloadProDos,
    Range([usize;2])
}

struct Operand {
    txt: String,
    num: Vec<u32>
}

impl Operand {
    fn txt(txt: String) -> Self {
        Self {
            txt,
            num: Vec::new()
        }
    }
    fn rel_addr(val: u32,bytes: usize,snip: &str) -> Self {
        Self {
            txt: snip.replace(&bytes.to_string(),&hex_from_val("$",val,2)),
            num: vec![val]
        }
    }
    fn abs_addr(val: u32,bytes: usize,snip: &str) -> Self {
        Self {
            txt: snip.replace(&bytes.to_string(),&hex_from_val("$",val,bytes)),
            num: vec![val]
        }
    }
    fn mov(val1: u8,val2: u8) -> Self {
        Self {
            txt: [hex_from_val("$",val1 as u32,1),",".to_string(),hex_from_val("$",val2 as u32,1)].concat(),
            num: vec![val1 as u32,val2 as u32]
        }
    }
}

struct DasmLine {
    address: usize,
    instruction: String,
    prefix: String,
    suffix: String,
    operand: Option<Operand>,
    references: Vec<usize>
}

impl DasmLine {
    fn new() -> Self {
        Self {
            address: 0,
            instruction: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            operand: None,
            references: Vec::new()
        }
    }
    fn basic(addr: usize, ins: String, op: Operand) -> Self {
        Self {
            address: addr,
            instruction: ins,
            prefix: String::new(),
            suffix: String::new(),
            operand: Some(op),
            references: Vec::new()
        }
    }
}

pub struct Disassembler {
    config: Settings,
    pc: Option<usize>,
    m8bit: bool,
    x8bit: bool,
    symbols: Arc<Symbols>,
    dasm_map: HashMap<u8,MachineOperation>,
    dasm_lines: Vec<DasmLine>,
    std_patt: regex::Regex,
    mov_patt: regex::Regex
}

fn u32_from_operand(slice: &[u8]) -> u32 {
    let mut bytes = [0,0,0,0];
    for i in 0..slice.len() {
        bytes[i] = slice[i]
    }
    u32::from_le_bytes(bytes)
}

fn hex_from_val(prefix: &str, val: u32, bytes: usize) -> String {
    let mut ans = String::new();
    let be_bytes = val.to_be_bytes();
    for i in 4-bytes..4 {
        ans += &format!("{:02X}",be_bytes[i]);
    }
    [prefix.to_string(),ans].concat()
}

fn is_alphanum(c: u8, off: u8) -> bool {
    c > 0x40 + off && c <= 0x5a + off || c > 0x60 + off && c <= 0x7a + off || c >= 0x30 + off && c <= 0x39 + off
}
fn probably_string(c: u8, off: u8) -> bool {
    // adds space, period, comma
    is_alphanum(c,off) || c == 32 + off || c == 44 + off || c == 46 + off
}

fn dos33_bload_range(img: &[u8]) -> Result<[usize;2],DYNERR> {
    let start = img[0xaa72] as usize + img[0xaa73] as usize * 0x100;
    let length = img[0xaa60] as usize + img[0xaa61] as usize * 0x100;
    let end = start + length;
    if end > img.len() {
        return Err(Box::new(lang::Error::OutOfRange));
    }
    Ok([start,end])
}
fn prodos_bload_range(img: &[u8]) -> Result<[usize;2],DYNERR> {
    let start = img[0xbeb9] as usize + img[0xbeba] as usize * 0x100;
    let length = img[0xbec8] as usize + img[0xbec9] as usize * 0x100;
    let end = start + length;
    if end > img.len() {
        return Err(Box::new(lang::Error::OutOfRange));
    }
    Ok([start,end])
}

impl Disassembler {
    pub fn new() -> Self {
        let book = OperationHandbook::new();
        Self {
            config: Settings::new(),
            pc: None,
            m8bit: true,
            x8bit: true,
            symbols: Arc::new(Symbols::new()),
            dasm_map: book.create_dasm_map(),
            dasm_lines: Vec::new(),
            std_patt: regex::Regex::new(r"[0-9]").expect(super::RCH),
            mov_patt: regex::Regex::new(r"[0-9][0-9]").expect(super::RCH)
        }
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
    pub fn set_program_counter(&mut self,pc: Option<usize>) {
        self.pc = pc;
    }
    pub fn set_mx(&mut self, m8bit: bool, x8bit: bool) {
        self.m8bit = m8bit;
        self.x8bit = x8bit;
    }
	fn modify(&self,s: &str) -> String {
		if let Some(sev) = self.config.flag.case_sensitive {
			if self.config.completions.lower_case && sev != lsp_types::DiagnosticSeverity::ERROR {
				return s.to_lowercase();
			}
		}
		return s.to_uppercase();
	}
    fn push_data_psop(&mut self, addr: usize, ins: String, dat: String) {
        self.dasm_lines.push(DasmLine::basic(addr, ins, Operand::txt(dat)));
    }
    fn push_data_pattern(&mut self, addr: usize, img: &[u8], length: usize, reps: usize) {
		if reps > 1 {
			self.push_data_psop(addr, self.modify("LUP"), reps.to_string());
		}
        let v: Vec<u8> = img[addr..addr+length].to_vec();
		self.push_data_psop(addr, self.modify("HEX"),v.encode_hex_upper());
		if reps > 1 {
			self.push_data_psop(addr, "--^".to_string(), "".to_string());
		}
    }
    /// * `neg` indicates the string that was found is negative ASCII
    /// * `s` is the string that was found
    /// * `lookahead` is the byte value that follows the string
    /// * returns 0, or 1 if a terminating byte was included
    fn push_string(&mut self, addr: usize, neg: bool, s: String, lookahead: Option<u8>) -> usize {
		let mut delim = match neg { true => "\"" , false => "'"};
		if s.starts_with(delim) {
			delim = match neg { true=> "&", false => "/"};
		}
        let off = match neg { true => 0, false => 128 };
		if lookahead.is_some() && lookahead.unwrap() == 0 {
            self.dasm_lines.push(DasmLine::basic(addr,self.modify("ASC"),Operand::txt([delim,&s,delim,",00"].concat())));
			return 1;
		}
		if lookahead.is_some() && probably_string(lookahead.unwrap(), off) {
            let term = String::from_utf8(vec![lookahead.unwrap() - off]).expect(super::RCH);
			self.dasm_lines.push(DasmLine::basic(addr,self.modify("DCI"),Operand::txt([delim,&s,&term,delim].concat())));
			return 1;
		}
        self.dasm_lines.push(DasmLine::basic(addr, self.modify("ASC"), Operand::txt([delim, &s, delim].concat())));
		return 0;
    }
	fn try_data_run(&mut self, img: &[u8], mut ptr: usize, end: usize) -> usize {
		// TODO list
		// * stop when we reach a previously defined branch destination
		// * handle inverted and flashing text
		let ptr0 = ptr;
		let mut pos_str: (usize, bool) = (0, true);
		let mut neg_str: (usize, bool) = (0, true);
		let mut uniform: (usize, bool) = (0, true);
		let mut pat2: (usize, bool) = (0, true);
		let mut pat4: (usize, bool) = (0, true);
		while (ptr0 == ptr || ptr < end) && ptr0 < end && (pos_str.1 || neg_str.1 || uniform.1 || pat2.1 || pat4.1) {
			let c = img[ptr];
			if pos_str.1 {
				if probably_string(c, 0) {
					pos_str.0 += 1;
				} else {
					pos_str.1 = false;
				}
			}
			if neg_str.1 {
				if probably_string(c, 128) {
					neg_str.0 += 1;
				} else {
					neg_str.1 = false;
				}
			}
			if uniform.1 && ptr > ptr0 && c == img[ptr - 1] {
				uniform.0 += 1;
			} else if ptr > ptr0 {
				uniform.1 = false;
			}
			if pat2.1 && ptr > ptr0 + 1 && c == img[ptr - 2] {
				pat2.0 += 1;
			} else if ptr > ptr0 + 1 {
				pat2.1 = false;
			}
			if pat4.1 && ptr > ptr0 + 3 && c == img[ptr - 4] {
				pat4.0 += 1;
			} else if ptr > ptr0 + 3 {
				pat4.1 = false;
			}
			ptr += 1;
		}
		if uniform.0 > 0 {
			uniform.0 += 1;
        }
		if pat2.0 > 0 {
			pat2.0 += 2;
			pat2.0 -= pat2.0 % 2;
		}
		if pat4.0 > 0 {
			pat4.0 += 4;
			pat4.0 -= pat4.0 % 4;
		}
		if uniform.0 > 0 && uniform.0 >= pat2.0 && uniform.0 >= pat4.0 && uniform.0 >= pos_str.0 && uniform.0 >= neg_str.0 {
			self.push_data_psop(ptr0, "DS".to_string(), [uniform.0.to_string(), ",$".to_string(), vec![img[ptr0]].encode_hex_upper()].concat());
			return uniform.0;
		}
		if pat2.0 > 0 && pat2.0 >= pat4.0 && pat2.0 >= pos_str.0 && pat2.0 >= neg_str.0 {
			self.push_data_pattern(ptr0, img, 2, pat2.0/2);
			return pat2.0;
		}
		if pat4.0 > pos_str.0 && pat4.0 > neg_str.0 {
			self.push_data_pattern(ptr0, img, 4, pat4.0/4);
			return pat4.0;
		}
		if pos_str.0 > neg_str.0 {
			let lookahead: Option<u8> = match ptr0 + pos_str.0 < end {
                true => Some(img[ptr0 + pos_str.0]),
                false => None
            };
            let v = img[ptr0..ptr0+pos_str.0].to_vec();
			let s = String::from_utf8(v).expect(super::RCH);
			return s.len() + self.push_string(ptr0, false, s, lookahead);
		}
		if neg_str.0 > 0 {
			let lookahead: Option<u8> = match ptr0 + neg_str.0 < end {
                true => Some(img[ptr0 + neg_str.0]),
                false => None
            };
            let mut v = img[ptr0..ptr0+neg_str.0].to_vec();
            v = v.iter().map(|x| x - 128).collect();
			let s = String::from_utf8(v).expect(super::RCH);
			return s.len() + self.push_string(ptr0, true, s, lookahead);
		}
		return 0
	}
    /// Check if we have a valid instruction and enough bytes left to accommodate it.
    /// This will return a `MachineOperation` that is modified for the current MX status.
    fn is_instruction(&self,val: u8,addr: usize,end: usize,proc: &ProcessorType) -> Option<(MachineOperation,usize)> {
        if let Some(op) = self.dasm_map.get(&val) {
            if op.processors.contains(proc) {
                if val!=0 || self.config.disassembly.brk {
                    let mut new_op = op.clone();
                    if let Some(_) = self.mov_patt.find(&op.operand_snippet) {
                        if addr + 1 + 2 <= end {
                            return Some((op.clone(),2));
                        } else {
                            return None;
                        }
                    }
                    if let Some(m) = self.std_patt.find(&op.operand_snippet) {
                        let mut operand_bytes = usize::from_str_radix(m.as_str(),10).expect(super::RCH);
                        if op.m_sensitive && !self.m8bit || op.x_sensitive && !self.x8bit {
                            operand_bytes += 1;
                            new_op.operand_snippet = "#2".to_string();
                        }
                        if addr + 1 + operand_bytes <= end {
                            return Some((new_op,operand_bytes));
                        } else {
                            return None;
                        }
                    }
                    return Some((new_op,0));
                }
            }
        }
        None
    }
    fn push_instruction(&mut self, img: &[u8], mut addr: usize, op: MachineOperation, operand_bytes: usize) -> Result<usize,DYNERR> {
        let mut new_line = DasmLine::new();
        new_line.address = addr;
        new_line.instruction = self.modify(&op.mnemonic);
        addr += 1;
        if self.mov_patt.is_match(&op.operand_snippet) {
            new_line.operand = Some(Operand::mov(img[addr+1],img[addr]));
            addr += 2;
        } else if operand_bytes > 0 {
            let mut val = u32_from_operand(&img[addr..addr+operand_bytes]) as usize;
            if op.relative {
                val = match OperationHandbook::rel_to_abs(addr-1, val, operand_bytes) {
                    Some(dest) => dest,
                    None => {
                        // if out of range interpret as data
                        log::debug!("branch out of bounds: {} + {}",addr-1,val);
                        self.push_data_pattern(addr-1,img,operand_bytes+1,1);
                        addr += operand_bytes;
                        return Ok(addr)
                    }
                };
            }
            if !op.operand_snippet.starts_with("#") {
                new_line.references.push(val);
            }
            if op.relative {
                new_line.operand = Some(Operand::rel_addr(val as u32,operand_bytes,&op.operand_snippet));
            } else {
                // suffix forcing appears to be the most universal
                new_line.suffix = match operand_bytes {
                    2 if val < 0x100 && op.abs_suffixable => ":".to_string(),
                    3 if val < 0x10000 && op.absl_suffixable => "L".to_string(),
                    _ => String::new()
                };
                new_line.prefix = match operand_bytes {
                    3 if op.absl_prefixable => ">".to_string(),
                    _ => String::new()
                };
                new_line.operand = Some(Operand::abs_addr(val as u32,operand_bytes,&self.modify(&op.operand_snippet)));
            }
            addr += operand_bytes;
        }
        self.dasm_lines.push(new_line);
        Ok(addr)
    }
    fn format_lines(&self,labeling: &str) -> String {
        let mut last_addr = usize::MAX;
        let widths = [self.config.columns.c1 as usize,self.config.columns.c2 as usize,self.config.columns.c3 as usize];
        let pc_bytes = match self.dasm_lines.iter().map(|x| x.address > 0xffff).collect::<Vec<bool>>().contains(&true) {
            true => 3,
            false => 2
        };
        let mut code = String::new();
        let mut labels = HashSet::new();
        // gather references
        let mut references = HashSet::new();
        for line in &self.dasm_lines {
            for r in &line.references {
                references.insert(*r);
            }
        }
		// determine labels
		for i in 0..self.dasm_lines.len()	{
			if labeling.contains("all") {
				labels.insert(self.dasm_lines[i].address);
            } else if labeling.contains("some") && (i==0 || references.contains(&self.dasm_lines[i].address)) {
				labels.insert(self.dasm_lines[i].address);
            }
		}
        // loop over lines
		for i in 0..self.dasm_lines.len() {
			let mut line = String::new();
			if labels.contains(&self.dasm_lines[i].address) && self.dasm_lines[i].address != last_addr {
				line += "_";
                line += &hex_from_val("",self.dasm_lines[i].address as u32,pc_bytes);
            }
            last_addr = self.dasm_lines[i].address;
			line.push(super::COLUMN_SEPARATOR);
            line += &self.dasm_lines[i].instruction;
            line += &self.dasm_lines[i].suffix;
            if let Some(operand) = &self.dasm_lines[i].operand {
                line.push(super::COLUMN_SEPARATOR);
                line += &self.dasm_lines[i].prefix;
                if operand.num.len() == 1 && labels.contains(&(operand.num[0] as usize)) && !operand.txt.starts_with("#") {
                    line += "_";
                    line += &hex_from_val("",operand.num[0] as u32,pc_bytes);
                } else {
                    line += &operand.txt;
                }
            }
			line = formatter::format_tokens(&line, &formatter::ColumnStyle::Variable, widths);
			code += &line;
            code += "\n";
		}
        code
    }
    /// Disassemble a range of bytes within `img`, which can be thought of as a RAM image.
    /// If the data source is a file, the data should be copied to `img` at the appropriate offset.
    /// In particular, the starting address will be taken as `range[0]`.
    /// Data sections are triggered by any failure to match an instruction.
	pub fn disassemble(&mut self, img: &[u8], range: DasmRange, proc: ProcessorType, labeling: &str) -> Result<String,DYNERR> {
        let addr_range = match range {
            DasmRange::All => [0,img.len()],
            DasmRange::LastBloadDos33 => dos33_bload_range(img)?,
            DasmRange::LastBloadProDos => prodos_bload_range(img)?,
            DasmRange::Range([beg,end]) => [beg,end]
        };
		let mut addr = addr_range[0];

		self.dasm_lines = Vec::new();
		while addr < addr_range[1] {
            if let Some((op,operand_bytes)) = self.is_instruction(img[addr],addr,addr_range[1],&proc) {
                addr = self.push_instruction(img, addr, op, operand_bytes)?;
			} else {
				let data_bytes = self.try_data_run(img, addr, addr_range[1]);
				addr += data_bytes;
				if data_bytes == 0 {
					self.push_data_psop(addr, self.modify("DFB"), hex_from_val("$",img[addr] as u32,1));
					addr += 1;
				}
			}
		}
		Ok(self.format_lines(labeling))
	}
    /// Disassemble as pure data.
    /// Various Merlin pseudo-operations are used to express the result.
    /// Purpose is to re-disassemble specified lines during an iterative disassembly process.
    /// This will label "some lines" if self.pc.is_some().
	pub fn disassemble_as_data(&mut self, buf: &[u8]) -> String {
		self.dasm_lines = Vec::new();
		let mut addr = match self.pc {
            Some(a) => a,
            None => 0x8000
        };
        let mut img: Vec<u8> = vec![0;addr];
        img.append(&mut buf.to_vec());
		while addr < img.len() {
			let data_bytes = self.try_data_run(&img, addr, img.len());
			addr += data_bytes;
            if data_bytes == 0 {
                self.push_data_psop(addr, self.modify("DFB"), hex_from_val("$",img[addr] as u32,1));
                addr += 1;
            }
		}
        self.format_lines(match self.pc { Some(_) => "some", None => "none"})
	}
    /// Disassemble with the most aggressive possible interpretation as code.
    /// Purpose is to re-disassemble specified lines during an iterative disassembly process.
    /// If self.pc.is_none() then ORG 0x8000 is assumed, otherwise self.pc is used and "some lines" are labeled.
	pub fn disassemble_as_code(&mut self, buf: &[u8]) -> String {
		self.dasm_lines = Vec::new();
        let mut addr = match self.pc {
            Some(pc) => pc,
            None => 0x8000
        };
        let mut img: Vec<u8> = vec![0;addr];
        img.append(&mut buf.to_vec());
		while addr < img.len() {
            if let Some((op,operand_bytes)) = self.is_instruction(img[addr],addr,img.len(),&self.symbols.processor) {
                if let Ok(a) = self.push_instruction(&img, addr, op, operand_bytes) {
                    addr = a;
                    continue
                }
			}
            self.push_data_psop(addr, self.modify("DFB"), hex_from_val("$",img[addr] as u32,1));
            addr += 1;
		}
        self.format_lines(match self.pc { Some(_) => "some", None => "none"})
	}
}