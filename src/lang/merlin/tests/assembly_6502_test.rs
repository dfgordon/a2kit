use super::super::assembly::Assembler;
use super::super::ProcessorType;
use super::super::Symbols;

fn test_assembler(hex: &str, code: String, pc: usize) {
    let line_count = code.lines().count();
    let img = hex::decode(hex).expect("hex error");
    let mut assembler = Assembler::new();
    let mut symbols = Symbols::new();
    symbols.processor = ProcessorType::_6502;
    assembler.use_shared_symbols(std::sync::Arc::new(symbols));
    let actual = assembler
        .spot_assemble(code, 0, line_count as isize, Some(pc))
        .expect("asm error");
    assert_eq!(actual, img);
}

mod forced_abs {
    #[test]
    fn adc() {
        let hex = "6d00007d0000790000";
        let mut test_code = String::new();
        test_code += "         ADC:  $0000\n";
        test_code += "         ADC:  $0000,X\n";
        test_code += "         ADC:  $0000,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn and() {
        let hex = "2d00003d0000390000";
        let mut test_code = String::new();
        test_code += "         AND:  $0000\n";
        test_code += "         AND:  $0000,X\n";
        test_code += "         AND:  $0000,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn lda() {
        let hex = "ad0000bd0000b90000";
        let mut test_code = String::new();
        test_code += "         LDA:  $0000\n";
        test_code += "         LDA:  $0000,X\n";
        test_code += "         LDA:  $0000,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn cpy() {
        let hex = "cc0000";
        let mut test_code = String::new();
        test_code += "         CPY:  $0000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn stx() {
        let hex = "8e0000";
        let mut test_code = String::new();
        test_code += "         STX:  $0000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn rol() {
        let hex = "2e00003e0000";
        let mut test_code = String::new();
        test_code += "         ROL:  $0000\n";
        test_code += "         ROL:  $0000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod octets {
    #[test]
    fn adc() {
        let hex = "6900650075006d00107d001079001061007100";
        let mut test_code = String::new();
        test_code += "         ADC   #$00\n";
        test_code += "         ADC   $00\n";
        test_code += "         ADC   $00,X\n";
        test_code += "         ADC   $1000\n";
        test_code += "         ADC   $1000,X\n";
        test_code += "         ADC   $1000,Y\n";
        test_code += "         ADC   ($00,X)\n";
        test_code += "         ADC   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn and() {
        let hex = "2900250035002d00103d001039001021003100";
        let mut test_code = String::new();
        test_code += "         AND   #$00\n";
        test_code += "         AND   $00\n";
        test_code += "         AND   $00,X\n";
        test_code += "         AND   $1000\n";
        test_code += "         AND   $1000,X\n";
        test_code += "         AND   $1000,Y\n";
        test_code += "         AND   ($00,X)\n";
        test_code += "         AND   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn cmp() {
        let hex = "c900c500d500cd0010dd0010d90010c100d100";
        let mut test_code = String::new();
        test_code += "         CMP   #$00\n";
        test_code += "         CMP   $00\n";
        test_code += "         CMP   $00,X\n";
        test_code += "         CMP   $1000\n";
        test_code += "         CMP   $1000,X\n";
        test_code += "         CMP   $1000,Y\n";
        test_code += "         CMP   ($00,X)\n";
        test_code += "         CMP   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn eor() {
        let hex = "4900450055004d00105d001059001041005100";
        let mut test_code = String::new();
        test_code += "         EOR   #$00\n";
        test_code += "         EOR   $00\n";
        test_code += "         EOR   $00,X\n";
        test_code += "         EOR   $1000\n";
        test_code += "         EOR   $1000,X\n";
        test_code += "         EOR   $1000,Y\n";
        test_code += "         EOR   ($00,X)\n";
        test_code += "         EOR   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn lda() {
        let hex = "a900a500b500ad0010bd0010b90010a100b100";
        let mut test_code = String::new();
        test_code += "         LDA   #$00\n";
        test_code += "         LDA   $00\n";
        test_code += "         LDA   $00,X\n";
        test_code += "         LDA   $1000\n";
        test_code += "         LDA   $1000,X\n";
        test_code += "         LDA   $1000,Y\n";
        test_code += "         LDA   ($00,X)\n";
        test_code += "         LDA   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ora() {
        let hex = "0900050015000d00101d001019001001001100";
        let mut test_code = String::new();
        test_code += "         ORA   #$00\n";
        test_code += "         ORA   $00\n";
        test_code += "         ORA   $00,X\n";
        test_code += "         ORA   $1000\n";
        test_code += "         ORA   $1000,X\n";
        test_code += "         ORA   $1000,Y\n";
        test_code += "         ORA   ($00,X)\n";
        test_code += "         ORA   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn sbc() {
        let hex = "e900e500f500ed0010fd0010f90010e100f100";
        let mut test_code = String::new();
        test_code += "         SBC   #$00\n";
        test_code += "         SBC   $00\n";
        test_code += "         SBC   $00,X\n";
        test_code += "         SBC   $1000\n";
        test_code += "         SBC   $1000,X\n";
        test_code += "         SBC   $1000,Y\n";
        test_code += "         SBC   ($00,X)\n";
        test_code += "         SBC   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod store_ops {
    #[test]
    fn sta() {
        let hex = "850095008d00109d001099001081009100";
        let mut test_code = String::new();
        test_code += "         STA   $00\n";
        test_code += "         STA   $00,X\n";
        test_code += "         STA   $1000\n";
        test_code += "         STA   $1000,X\n";
        test_code += "         STA   $1000,Y\n";
        test_code += "         STA   ($00,X)\n";
        test_code += "         STA   ($00),Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn stx() {
        let hex = "860096008e0010";
        let mut test_code = String::new();
        test_code += "         STX   $00\n";
        test_code += "         STX   $00,Y\n";
        test_code += "         STX   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn sty() {
        let hex = "840094008c0010";
        let mut test_code = String::new();
        test_code += "         STY   $00\n";
        test_code += "         STY   $00,X\n";
        test_code += "         STY   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod index_ops {    
    #[test]
    fn cpx() {
        let hex = "e000e400ec0010";
        let mut test_code = String::new();
        test_code += "         CPX   #$00\n";
        test_code += "         CPX   $00\n";
        test_code += "         CPX   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn cpy() {
        let hex = "c000c400cc0010";
        let mut test_code = String::new();
        test_code += "         CPY   #$00\n";
        test_code += "         CPY   $00\n";
        test_code += "         CPY   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ldx() {
        let hex = "a200a600b600ae0010be0010";
        let mut test_code = String::new();
        test_code += "         LDX   #$00\n";
        test_code += "         LDX   $00\n";
        test_code += "         LDX   $00,Y\n";
        test_code += "         LDX   $1000\n";
        test_code += "         LDX   $1000,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ldy() {
        let hex = "a000a400b400ac0010bc0010";
        let mut test_code = String::new();
        test_code += "         LDY   #$00\n";
        test_code += "         LDY   $00\n";
        test_code += "         LDY   $00,X\n";
        test_code += "         LDY   $1000\n";
        test_code += "         LDY   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod branching {    
    #[test]
    fn forward_branch() {
        let hex = "907fb010f0003000d000100050007000";
        let mut test_code = String::new();
        test_code += "         BCC   $0381\n";
        test_code += "         BCS   $0314\n";
        test_code += "         BEQ   $0306\n";
        test_code += "         BMI   $0308\n";
        test_code += "         BNE   $030A\n";
        test_code += "         BPL   $030C\n";
        test_code += "         BVC   $030E\n";
        test_code += "         BVS   $0310\n";
        super::test_assembler(hex, test_code, 768);
    }
    #[test]
    fn reverse_branch() {
        let hex = "9000b0fcf0fc30fcd0fc10fc50fc70fc";
        let mut test_code = String::new();
        test_code += "         BCC   $0002\n";
        test_code += "         BCS   $0000\n";
        test_code += "         BEQ   $0002\n";
        test_code += "         BMI   $0004\n";
        test_code += "         BNE   $0006\n";
        test_code += "         BPL   $0008\n";
        test_code += "         BVC   $000A\n";
        test_code += "         BVS   $000C\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn jumping() {
        let hex = "4c00106c00102000104060";
        let mut test_code = String::new();
        test_code += "         JMP   $1000\n";
        test_code += "         JMP   ($1000)\n";
        test_code += "         JSR   $1000\n";
        test_code += "         RTI\n";
        test_code += "         RTS\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod short_ops {    
    #[test]
    fn status() {
        let hex = "18d858b838f878";
        let mut test_code = String::new();
        test_code += "         CLC\n";
        test_code += "         CLD\n";
        test_code += "         CLI\n";
        test_code += "         CLV\n";
        test_code += "         SEC\n";
        test_code += "         SED\n";
        test_code += "         SEI\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn stack() {
        let hex = "48086828";
        let mut test_code = String::new();
        test_code += "         PHA\n";
        test_code += "         PHP\n";
        test_code += "         PLA\n";
        test_code += "         PLP\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn transfer() {
        let hex = "aaa8ba8a9a98";
        let mut test_code = String::new();
        test_code += "         TAX\n";
        test_code += "         TAY\n";
        test_code += "         TSX\n";
        test_code += "         TXA\n";
        test_code += "         TXS\n";
        test_code += "         TYA\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn increment() {
        let hex = "ca88e600f600ee0010fe0010e8c8";
        let mut test_code = String::new();
        test_code += "         DEX\n";
        test_code += "         DEY\n";
        test_code += "         INC   $00\n";
        test_code += "         INC   $00,X\n";
        test_code += "         INC   $1000\n";
        test_code += "         INC   $1000,X\n";
        test_code += "         INX\n";
        test_code += "         INY\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn brk() {
        let hex = "0000EA";
        let mut test_code = String::new();
        test_code += "         BRK   #$00\n";
        test_code += "         NOP\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn no_brk() {
        let hex = "0000EA";
        let mut test_code = String::new();
        test_code += "         DS    2,$00\n";
        test_code += "         NOP\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod bitwise {    
    #[test]
    fn asl() {
        let hex = "0a060016000e00101e0010";
        let mut test_code = String::new();
        test_code += "         ASL\n";
        test_code += "         ASL   $00\n";
        test_code += "         ASL   $00,X\n";
        test_code += "         ASL   $1000\n";
        test_code += "         ASL   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn bit() {
        let hex = "24002c0010";
        let mut test_code = String::new();
        test_code += "         BIT   $00\n";
        test_code += "         BIT   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn lsr() {
        let hex = "4a460056004e00105e0010";
        let mut test_code = String::new();
        test_code += "         LSR\n";
        test_code += "         LSR   $00\n";
        test_code += "         LSR   $00,X\n";
        test_code += "         LSR   $1000\n";
        test_code += "         LSR   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn rol() {
        let hex = "2a260036002e00103e0010";
        let mut test_code = String::new();
        test_code += "         ROL\n";
        test_code += "         ROL   $00\n";
        test_code += "         ROL   $00,X\n";
        test_code += "         ROL   $1000\n";
        test_code += "         ROL   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ror() {
        let hex = "6a660076006e00107e0010";
        let mut test_code = String::new();
        test_code += "         ROR\n";
        test_code += "         ROR   $00\n";
        test_code += "         ROR   $00,X\n";
        test_code += "         ROR   $1000\n";
        test_code += "         ROR   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
}

// Check that ZP addresses are padded in cases where
// there is no ZP addressing mode available.
mod requires_padding {
    #[test]
    fn adc() {
        let hex = "791000";
        let mut test_code = String::new();
        test_code += "         ADC   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn and() {
        let hex = "391000";
        let mut test_code = String::new();
        test_code += "         AND   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn cmp() {
        let hex = "d91000";
        let mut test_code = String::new();
        test_code += "         CMP   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn eor() {
        let hex = "591000";
        let mut test_code = String::new();
        test_code += "         EOR   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn lda() {
        let hex = "b91000";
        let mut test_code = String::new();
        test_code += "         LDA   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ora() {
        let hex = "191000";
        let mut test_code = String::new();
        test_code += "         ORA   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn sbc() {
        let hex = "f91000";
        let mut test_code = String::new();
        test_code += "         SBC   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn sta() {
        let hex = "991000";
        let mut test_code = String::new();
        test_code += "         STA   $10,Y\n";
        super::test_assembler(hex, test_code, 0);
    }
}
