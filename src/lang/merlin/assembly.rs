//! Assembler for any Merlin version.
//! 
//! Currently we provide only "spot assembly," which is useful
//! for overriding the disassembler's preference for code over data.  This operates
//! on lines where neither the program counter nor the symbol values are needed, and makes
//! an assumption that labels of the form `_XXXX` are hex strings giving a literal address.
//! 
//! If assembly fails an error should be returned gracefully.

use std::sync::Arc;
use super::settings::Settings;
use super::Symbols;
use super::handbook::operations::OperationHandbook;
use super::MerlinVersion;
use thiserror::Error;
use crate::lang::merlin::{Operation,ProcessorType};
use crate::lang::{node_radix, node_text, Navigation, Navigate};
use crate::{STDRESULT,DYNERR};


/// closely parallels Merlin 8/16 error messages
#[derive(Error,Debug)]
pub enum Error {
    #[error("Bad address mode")]
    BadAddressMode,
    #[error("Branch jumps too far")]
    BadBranch,
    #[error("EXT or ENT out of place")]
    BadExternal,
    #[error("Required label missing")]
    MissingLabel,
    #[error("OBJ out of place or bad value")]
    BadObj,
    #[error("ORG out of place")]
    BadOrg,
    #[error("PUT out of place")]
    BadPut,
    #[error("REL out of place")]
    BadRel,
    #[error("SAV out of place")]
    BadSav,
    #[error("bad macro arguments")]
    BadArgs,
    #[error("duplicate symbol")]
    DuplicateSymbol,
    #[error("file type mismatch")]
    FileTypeMismatch,
    #[error("cannot evaluate expression")]
    ExpressionEvaluation,
    #[error("illegal forward reference")]
    ForwardRef,
    #[error("illegal relative address")]
    IllegalRelAddr,
    #[error("latest pass changed value")]
    Misalignment,
    #[error("nesting too deep")]
    Nesting,
    #[error("undefined macro")]
    UndefinedMacro,
    #[error("unresolved external")]
    UnresolvedExt,
    #[error("out of range")]
    OutOfRange,
    #[error("syntax")]
    Syntax,
    #[error("two or more EXT in operand")]
    TwoExt,
    #[error("undefined label")]
    UndefinedLabel,
    #[error("too many externals")]
    Externals,
    #[error("program counter could not be resolved")]
    UnresolvedProgramCounter,
    #[error("unsupported (pseudo) operations or arguments")]
    CannotAssemble
}

pub struct Assembler
{
    parser: tree_sitter::Parser,
    row: isize,
    col: isize,
    config: Settings,
    op_handbook: OperationHandbook,
    symbols: Arc<Symbols>,
    code: Vec<u8>,
    line: String,
    m8bit: bool,
    x8bit: bool,
    pc: Option<usize>
}

impl Assembler {
	pub fn new() -> Self
	{
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_merlin6502::language()).expect("failed to load language");
        Self {
            parser,
            row: 0,
            col: 0,
            config: Settings::new(),
            op_handbook: OperationHandbook::new(),
            symbols: Arc::new(Symbols::new()),
            code: Vec::new(),
            line: String::new(),
            m8bit: true,
            x8bit: true,
            pc: None
        }
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
    pub fn set_mx(&mut self, m8bit: bool, x8bit: bool) {
        self.m8bit = m8bit;
        self.x8bit = x8bit;
    }
    pub fn set_program_counter(&mut self,pc: usize) {
        self.pc = Some(pc);
    }
    fn prefix_shift(prefix: &str) -> usize {
        match prefix {
            "#>" | ">" => 1,
            "#^" | "^" => 2,
            _ => 0
        }
    }
    /// Works for either immediate mode or data-like modes
    fn eval_imm_prefix(is16bit: bool,prefix: &str,beg: &mut usize,end: &mut usize) {
        // Confirmed Merlin 32 handles the same way.
        if is16bit {
            *beg = 0;
            *end = 2;
        } else {
            *beg = 0;
            *end = 1;
        }
        *beg += Self::prefix_shift(prefix);
        *end += Self::prefix_shift(prefix);
    }
    /// Suffix effects must be applied before calling this
    fn eval_addr_prefix(prefix: &str,beg: &mut usize,end: &mut usize) {
        // Confirmed Merlin 32 handles the same way, provided suffix applied first.
		if prefix == ">" {
            *beg = 0;
            *end = 3;
		} else if *end < 3 && (prefix == "|" || prefix == "!") {
            *beg = 0;
            *end = 2;
        }
    }
    /// Evaluate data pseudo-op arguments and arguments to PEA, SEP, REP, MVN, MVP.
    /// Result will include the effects of prefix modifiers.
    fn eval_data(&self, data: &tree_sitter::Node,byte_count: usize,big_endian: bool) -> Result<i64,DYNERR> {
        let (prefix,num) = match data.named_child_count() {
            1 => (String::new(),data.named_child(0).unwrap()),
            2 => (node_text(&data.named_child(0).unwrap(),&self.line), data.named_child(1).unwrap()),
            _ => return Err(Box::new(Error::Syntax))
        };
        let mut val = self.eval_expr(&num, &self.line)?;
        if big_endian {
            val <<= 32 - byte_count * 8 - Self::prefix_shift(&prefix) * 8;
        } else {
            val >>= Self::prefix_shift(&prefix) * 8;
        }
        Ok(val)
    }
    fn push_data(&mut self, mut val: i64, byte_count: usize, big_endian: bool) {
        if big_endian {
            for _i in 0..byte_count {
                self.code.push(((val & 0xff000000) >> 24) as u8);
                val <<= 8;
            }
        } else {
            for _i in 0..byte_count {
                self.code.push((val & 0xff) as u8);
                val >>= 8;
            }
        }
        if let Some(pc) = self.pc.as_mut() {
            *pc += byte_count;
        }
}
    fn push_data_list(&mut self,arg: &tree_sitter::Node,byte_count: usize,big_endian: bool) -> STDRESULT {
        let mut curs = arg.walk();
        for data in arg.named_children(&mut curs) {
            let val = self.eval_data(&data, byte_count, big_endian)?;
            self.push_data(val,byte_count,big_endian);
        }
        Ok(())
    }
	/// Push the instruction, can be affected by mx, updates PC if defined.
    /// * op - the operation
    /// * op_node - parsing node of the operation, such as op_lda, op_eor, etc.
	/// * mode_node - parsing node such as imm, addr, addr_x, etc.
	/// * val - raw value of the operand expression (e.g. if a branch pass the absolute address)
    /// * prefix - expression modifier such as #, #<, etc.
	/// returns success or error
	fn push_instruction(&mut self, op: &Operation, op_node: &tree_sitter::Node, mode_node: &tree_sitter::Node, val: u32, prefix: &str) -> STDRESULT {
		let mut val_bytes = u32::to_le_bytes(val);
		let mut beg = 0;
		let mut end = 1;
		if val_bytes[1] > 0 {
			end = 2;
        }
        let suffix = match op_node.named_child(0) {
            Some(s) => node_text(&s,&self.line),
            None => "".to_string()
        };

        // Handle mnemonic suffix forcing absolute or long addressing,
        // this must precede the prefix handling.
        if end == 1 {
            if self.symbols.assembler==MerlinVersion::Merlin8 {
                if suffix.len() > 0 && suffix != "D" && suffix != "d" {
                    end = 2;
                }
            } else {
                if suffix.len() > 0 && suffix != "L" && suffix != "l" {
                    end = 2;
                }
            }
        }
        if self.symbols.assembler!=MerlinVersion::Merlin8 {
            if suffix.len() > 0 && (suffix == "l" || suffix == "L") {
                end = 3;
            }
        }

        // Handle prefix modifiers and special cases
        let is16bit = !self.x8bit && op.x_sensitive || !self.m8bit && op.m_sensitive;
        if mode_node.kind()=="data" {
            if op_node.kind()=="op_pea" {
                Self::eval_imm_prefix(true, prefix, &mut beg, &mut end);
            } else {
                Self::eval_imm_prefix(false, prefix, &mut beg, &mut end);
            }
        } else if op_node.kind()=="op_brl" {
            beg = 0;
            end = 2;
        } else if op_node.kind()=="op_jml" && mode_node.kind()=="addr" {
            beg = 0;
            end = 3;
        } else if op_node.kind()=="op_jml" && mode_node.kind()=="iaddr" {
            beg = 0;
            end = 2;
        } else if op_node.kind()=="op_jsl" {
            beg = 0;
            end = 3;
        } else if prefix.starts_with("#") {
            Self::eval_imm_prefix(is16bit, prefix, &mut beg, &mut end);
        } else {
            Self::eval_addr_prefix( prefix, &mut beg, &mut end);
		}

        if self.symbols.processor==ProcessorType::_6502 || self.symbols.processor==ProcessorType::_65c02 {
            if end > beg + 2 {
                return Err(Box::new(Error::OutOfRange));
            }
        }

        let mode = op.get_address_mode(mode_node.kind(), end-beg)?;
        self.code.push(mode.code as u8);
        if mode.mnemonic=="rel" {
            let abs = val_bytes[0] as isize + 0x100 * val_bytes[1] as isize;
            if let Some(pc) = self.pc {
                let rel = match abs - (pc as isize + 2) {
                    x if x >= 0 => x,
                    x => x + 0x100
                };
                if rel < 0 || rel > u8::MAX as isize {
                    return Err(Box::new(Error::BadBranch));
                }
                val_bytes = u32::to_le_bytes(rel as u32);
                beg = 0;
                end = 1;
            } else {
                return Err(Box::new(Error::UnresolvedProgramCounter));
            }
        } else if mode.mnemonic=="rell" {
            let abs = val_bytes[0] as isize + 0x100 * val_bytes[1] as isize + 0x10000 * val_bytes[2] as isize;
            if let Some(pc) = self.pc {
                let rel = match abs - (pc as isize + 3) {
                    x if x >= 0 => x,
                    x => x + 0x10000
                };
                if rel < 0 || rel > u16::MAX as isize {
                    return Err(Box::new(Error::BadBranch));
                }
                val_bytes = u32::to_le_bytes(rel as u32);
                beg = 0;
                end = 2;
            } else {
                return Err(Box::new(Error::UnresolvedProgramCounter));
            }
        }
        for i in beg..end {
            self.code.push(val_bytes[i]);
        }
        if let Some(pc) = self.pc.as_mut() {
            *pc += 1 + end - beg;
        }
        Ok(())
	}
	fn eval_expr(&self, node: &tree_sitter::Node, source: &str) -> Result<i64,DYNERR> {
        match node.kind() {
            "num" => {
                match node_radix(node, source, "$", "%") {
                    Some(v) => Ok(v),
                    None => Err(Box::new(Error::ExpressionEvaluation))
                }
            },
            "label_ref" => {
                let txt = node_text(node, source);
                match (txt.starts_with("_"),node_radix(node, source, "_", "%")) {
                    (true,Some(v)) => Ok(v),
                    _ => Err(Box::new(Error::ExpressionEvaluation))
                }
            },
            _ => Err(Box::new(Error::ExpressionEvaluation))
        }
	}
    /// Used to evaluate the several string-like pseudo-ops.  Within the dstring segments,
    /// the bytes are subjected to 4 transformations in order: clos, signed, dci, reverse.
    /// Hex segments are handled according to Merlin 16 rules.  The `node` should be the `arg_` node.
    fn push_strings<F>(&mut self,node: &tree_sitter::Node, mut clos: F, signed: bool, dci: bool, reverse: bool, prefix_len: usize) -> STDRESULT
    where F: FnMut(u8) -> u8
    {
        let mut ans = Vec::new();
        let mut curs = node.walk();
        for child in node.named_children(&mut curs) {
            match child.kind() {
                "dstring" => {
                    let mut v = node_text(&child,&self.line).bytes().map(&mut clos).collect::<Vec<u8>>();
                    let len = v.len();
                    if signed && len > 0 && v[0] < b'\'' {
                        v = v.iter().map(|x| x | 0x80).collect::<Vec<u8>>();
                    }
                    if len < 2 || v[0] != v[len-1] {
                        return Err(Box::new(Error::Syntax));
                    } else if len > 2 {
                        if dci {
                            v[len-2] = v[len-2] ^ 0x80;
                        }
                        if reverse {
                            v.reverse();
                        }
                        if prefix_len==1 {
                            if len < 0x102 {
                                ans.push((len-2) as u8);
                            } else {
                                return Err(Box::new(Error::OutOfRange));
                            }
                        }
                        if prefix_len==2 {
                            if len < 0x10002 {
                                ans.append(&mut u16::to_le_bytes((len-2) as u16).to_vec());
                            } else {
                                return Err(Box::new(Error::OutOfRange));
                            }
                        }
                        ans.append(&mut v[1..len-1].to_vec());
                    }
                },
                "hex_data" => {
                    ans.append(&mut hex::decode(node_text(&child,&self.line))?);
                },
                _ => return Err(Box::new(Error::CannotAssemble))
            }
        }
        if let Some(pc) = self.pc.as_mut() {
            *pc += ans.len();
        }
        self.code.append(&mut ans);
        Ok(())
    }
    /// Try to assemble lines in a circumstance where the symbol values and program counter
    /// are not necessarily known.  The spot assembler will proceed as far as it can with
    /// whatever information is available, and error out if it hits something that cannot
    /// be handled (e.g. a relative branch with unknown PC).
	pub fn spot_assemble(&mut self, txt: String, beg: isize, end: isize) -> Result<Vec<u8>,DYNERR> {
        self.code = Vec::new();
		self.row = 0;
		for line in txt.lines() {
            if self.row < beg {
                self.row += 1;
                continue;
            }
            if self.row >= end {
                break;
            }
			self.col = 0;
			self.line = self.symbols.adjust_line(self.row, line, "\n");
			if self.line.starts_with(super::CALL_TOK) {
				// ASSUMPTION is col will be a byte offset and LSP position encoding is utf-16
				self.col = -2*(super::CALL_TOK.len_utf16() as isize);
			}
			if let Some(tree) = self.parser.parse(&self.line,None) {
				self.walk(&tree)?;
			}
			self.row += 1;
		}
        Ok(self.code.clone())
    }
}

impl Navigate for Assembler {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if ["macro_call","program_counter","comment","heading"].contains(&curs.node().kind()) {
			return Err(Box::new(Error::CannotAssemble));
		}
		if curs.node().has_error() {
			return Err(Box::new(Error::Syntax));
		}

		if curs.node().kind().starts_with("op_") {
            let txt = node_text(&curs.node(), &self.line);
            if let Some(op) = self.op_handbook.get(&txt) {
                match curs.node().next_named_sibling() {
                    Some(nxt) => {
                        if !nxt.kind().starts_with("arg_") {
                            return Err(Box::new(Error::Syntax));
                        }
                        if let Some(mode_node) = nxt.named_child(0) {
                            if mode_node.kind() == "xyc" {
                                self.code.push(op.modes[0].code as u8);
                                self.push_data_list(&mode_node, 1, false)?;
                                if self.code.len() > 2 {
                                    let dst = self.code.pop().unwrap();
                                    let src = self.code.pop().unwrap();
                                    self.code.push(dst);
                                    self.code.push(src);
                                    return Ok(Navigation::Exit);
                                }
                                return Err(Box::new(Error::Syntax));
                            }
                            let (prefix,expr) = match (mode_node.named_child(0),mode_node.named_child(1)) {
                                (Some(n1),None) => (None,n1),
                                (Some(n1),Some(n2)) if n2.kind() == "mode" => (None,n1),
                                (Some(n1),Some(n2)) => (Some(n1),n2),
                                _ => return Err(Box::new(Error::Syntax))
                            };
                            let prefix_txt = match prefix {
                                Some(n) => node_text(&n,&self.line),
                                None => "".to_string()
                            };
                            match self.eval_expr(&expr,&self.line) {
                                Ok(val) => {
                                    self.push_instruction(&op, &curs.node(),&mode_node, val as u32, &prefix_txt)?;
                                    return Ok(Navigation::Exit);
                                },
                                Err(e) => return Err(e)
                            }
                        } else {
                            return Err(Box::new(Error::Syntax));
                        }
                    },
                    None => {
                        for mode in op.modes {
                            if ["accum","impl","s"].contains(&mode.mnemonic.as_str()) {
                                self.code.push(mode.code as u8);
                                return Ok(Navigation::Exit);
                            }
                        }
                    }
                }
            }
		}

		if curs.node().kind().starts_with("psop_") {
            match curs.node().next_named_sibling() {
                Some(arg) => {
                    match arg.kind() {
                        "arg_mx" => {
                            if let Some(child) = arg.named_child(0) {
                                // TODO: should we allow prefixes here?
                                match self.eval_expr(&child,&self.line) {
                                    Ok(val) => {
                                        self.m8bit = val & 0b10 > 0;
                                        self.x8bit = val & 0b01 > 0;
                                        return Ok(Navigation::Exit); 
                                    },
                                    Err(e) => return Err(e)
                                }
                            }
                            return Err(Box::new(Error::Syntax));
                        },
                        "arg_asc" => {
                            if let Some(child) = arg.named_child(0) {
                                if child.kind() == "num_str_prefix" {
                                    return Err(Box::new(Error::CannotAssemble));
                                }
                            }
                            self.push_strings(&arg, |x| x, true, false, false, 0)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_dci" => {
                            self.push_strings(&arg, |x| x, true, true, false, 0)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_inv" => {
                            let inv_closure = match self.config.version {
                                MerlinVersion::Merlin8 | MerlinVersion::Merlin16 => | x | -> u8 {
                                    x & 0x3f
                                },
                                MerlinVersion::Merlin16Plus | MerlinVersion::Merlin32 => | x | -> u8 {
                                    if x >= 0x60 {
                                        x
                                    } else {
                                        x & 0x3f
                                    }
                                }
                            };
                            self.push_strings(&arg, inv_closure, false, false, false, 0)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_fls" => {
                            self.push_strings(&arg, |x| x | 0x40, false, false, false, 0)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_rev" => {
                            self.push_strings(&arg, |x| x, true, false, true, 0)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_str" => {
                            self.push_strings(&arg, |x| x, true, false, false, 1)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_strl" => {
                            self.push_strings(&arg, |x| x, true, false, false, 2)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_flo" => {
                            return Err(Box::new(Error::CannotAssemble));
                        },
                        "arg_dfb" => {
                            // list of expressions that will go in as bytes
                            self.push_data_list(&arg,1,false)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_da" => {
                            // list of expressions that will go in as 16 bit LE
                            self.push_data_list(&arg,2,false)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_ddb" => {
                            // list of expressions that will go in as 16 bit BE
                            self.push_data_list(&arg, 2, true)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_adr" => {
                            // list of expressions that will go in as 24 bit LE
                            self.push_data_list(&arg, 3, false)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_adrl" => {
                            // list of expressions that will go in as 32 bit LE
                            self.push_data_list(&arg, 4, false)?;
                            return Ok(Navigation::Exit);
                        },
                        "arg_hex" => {
                            let txt = node_text(&arg, &self.line).replace(",","");
                            let mut hex = hex::decode(&txt)?;
                            self.code.append(&mut hex);
                            return Ok(Navigation::Exit);
                        },
                        "arg_ds" => {
                            let mut curs = arg.walk();
                            let mut iter = arg.named_children(&mut curs);
                            let mut maybe_reps: Option<usize> = None;
                            while let Some(child) = iter.next() {
                                if child.kind() == "new_page" {
                                    return Err(Box::new(Error::CannotAssemble));
                                } else if child.kind() == "data" {
                                    let val = self.eval_data(&child, 1, false)?;
                                    if let Some(reps) = maybe_reps {
                                        for _i in 0..reps {
                                            self.push_data(val,1,false);
                                        }
                                    } else {
                                        let shifted = u16::try_from(val)?;
                                        maybe_reps = Some(shifted as usize);
                                    }
                                } else {
                                    return Err(Box::new(Error::Syntax));
                                }
                            }
                            return Ok(Navigation::Exit);
                        }
                        _ => return Err(Box::new(Error::CannotAssemble))
                    }

                },
                None => return Ok(Navigation::Exit)
            }
		}

		Ok(Navigation::GotoChild)
    }
}