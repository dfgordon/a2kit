use crate::lang::merlin::handbook::{operations::OperationHandbook, pseudo_ops::PseudoOperationHandbook};
use crate::lang::merlin::ProcessorType;

pub struct StatementHovers {
	op_book: OperationHandbook,
	psop_book: PseudoOperationHandbook
}

impl StatementHovers {
    pub fn new() -> Self {
        let op_book = OperationHandbook::new();
        let psop_book = PseudoOperationHandbook::new();
        Self {
            op_book,
            psop_book
        }
    }
    pub fn get_op(&self,node_kind: &str) -> Option<String> {
        if node_kind.starts_with("op_") && node_kind.len() > 3 {
            if let Some(op) = self.op_book.get(&node_kind[3..]) {
                let mut ans = format!("`{}`\n",node_kind[3..].to_uppercase());
                ans += "\n\n---\n\n";
                ans += &op.desc;
                ans += "\n\n---\n\n";
                let mut table = "addr|cyc|op|xc\n---|---:|---|---\n".to_string();
                for mode in op.modes {
                    table += &format!("{:8}",mode.mnemonic);
                    table += "|";
                    table += &format!("{}",mode.cycles);
                    table += "|";
                    table += &format!("${:02X}",mode.code);
                    table += "|";
                    let mut xc = String::new();
                    if !mode.processors.contains(&ProcessorType::_6502) {
                        xc += "*";
                    }
                    if !mode.processors.contains(&ProcessorType::_65c02) {
                        xc += "*";
                    }
                    table += &xc;
                    table += "\n";
                }
                ans += &table;
                ans += "\n\n---\n\n";
                ans += "status register";
                let mut status = "N|V|M|X|D|I|Z|C\n---|---|---|---|---|---|---|---\n".to_string();
                for i in 0..8 {
                    status += &op.status[i..i+1];
                    status += "|";
                }
                ans += "\n\n---\n\n";
                ans += &status;
                return Some(ans);
            }
        }
        None
    }
    pub fn get_psop(&self,node_kind: &str) -> Option<String> {
        if node_kind.starts_with("psop_") && node_kind.len() > 5 {
            // handle end_lup specially
            let key = match node_kind {
                "psop_end_lup" => "--^",
                _ => &node_kind[5..]
            };
            if let Some(psop) = self.psop_book.get(key) {
                let mut ans = format!("`{}`",key.to_uppercase());
                for alt in psop.alt {
                    ans += &format!(" or `{}`",alt.to_uppercase());
                }
                ans += "\n\n---\n\n";
                ans += &psop.desc;
                if psop.eg.len() > 0 {
                    ans += "\n\n---\n\n";
                    ans += "#### examples\n\n    ";
                    for eg in psop.eg {
                        ans += "\n";
                        ans += &format!("    {}",eg);
                    }
                }
                if let Some(cav) = psop.caveat {
                    ans += "\n\n---\n\n";
                    ans += &format!("n.b. {}",cav);
                }
                ans += "\n\n---\n\n";
                ans += "Merlin versions: ";
                for vit in psop.version {
                    ans += &format!("{}, ",vit);
                }
                if ans.ends_with(", ") {
                    ans.pop();
                    ans.pop();
                }
                return Some(ans)
            }
        }
        None
    }
}
