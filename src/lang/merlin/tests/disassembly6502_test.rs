use super::super::disassembly::{DasmRange,Disassembler};
use super::super::ProcessorType;
use super::super::settings::Settings;

fn test_disassembler(hex: &str, expected: &str, brk: bool) {
    let img = hex::decode(hex).expect("hex error");
    let mut disassembler = Disassembler::new();
    let mut config = Settings::new();
    config.disassembly.brk = brk;
    disassembler.set_config(config);
    let actual = disassembler.disassemble(
        &img,
        DasmRange::All,
        ProcessorType::_6502,
        "none").expect("dasm error");
    assert_eq!(actual,expected);
}

mod forced_abs {
    #[test]
    fn adc() {
        let hex = "6d00007d0000790000";
        let mut expected = String::new();
        expected += "         ADC:  $0000\n";
        expected += "         ADC:  $0000,X\n";
        expected += "         ADC:  $0000,Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn and() {
        let hex = "2d00003d0000390000";
        let mut expected = String::new();
        expected += "         AND:  $0000\n";
        expected += "         AND:  $0000,X\n";
        expected += "         AND:  $0000,Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn lda() {
        let hex = "ad0000bd0000b90000";
        let mut expected = String::new();
        expected += "         LDA:  $0000\n";
        expected += "         LDA:  $0000,X\n";
        expected += "         LDA:  $0000,Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn cpy() {
        let hex = "cc0000";
        let mut expected = String::new();
        expected += "         CPY:  $0000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn stx() {
        let hex = "8e0000";
        let mut expected = String::new();
        expected += "         STX:  $0000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn rol() {
        let hex = "2e00003e0000";
        let mut expected = String::new();
        expected += "         ROL:  $0000\n";
        expected += "         ROL:  $0000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
}

mod octets {
    #[test]
    fn adc() {
        let hex = "6900650075006d00107d001079001061007100";
        let mut expected = String::new();
        expected += "         ADC   #$00\n";
        expected += "         ADC   $00\n";
        expected += "         ADC   $00,X\n";
        expected += "         ADC   $1000\n";
        expected += "         ADC   $1000,X\n";
        expected += "         ADC   $1000,Y\n";
        expected += "         ADC   ($00,X)\n";
        expected += "         ADC   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn and() {
        let hex = "2900250035002d00103d001039001021003100";
        let mut expected = String::new();
        expected += "         AND   #$00\n";
        expected += "         AND   $00\n";
        expected += "         AND   $00,X\n";
        expected += "         AND   $1000\n";
        expected += "         AND   $1000,X\n";
        expected += "         AND   $1000,Y\n";
        expected += "         AND   ($00,X)\n";
        expected += "         AND   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn cmp() {
        let hex = "c900c500d500cd0010dd0010d90010c100d100";
        let mut expected = String::new();
        expected += "         CMP   #$00\n";
        expected += "         CMP   $00\n";
        expected += "         CMP   $00,X\n";
        expected += "         CMP   $1000\n";
        expected += "         CMP   $1000,X\n";
        expected += "         CMP   $1000,Y\n";
        expected += "         CMP   ($00,X)\n";
        expected += "         CMP   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn eor() {
        let hex = "4900450055004d00105d001059001041005100";
        let mut expected = String::new();
        expected += "         EOR   #$00\n";
        expected += "         EOR   $00\n";
        expected += "         EOR   $00,X\n";
        expected += "         EOR   $1000\n";
        expected += "         EOR   $1000,X\n";
        expected += "         EOR   $1000,Y\n";
        expected += "         EOR   ($00,X)\n";
        expected += "         EOR   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn lda() {
        let hex = "a900a500b500ad0010bd0010b90010a100b100";
        let mut expected = String::new();
        expected += "         LDA   #$00\n";
        expected += "         LDA   $00\n";
        expected += "         LDA   $00,X\n";
        expected += "         LDA   $1000\n";
        expected += "         LDA   $1000,X\n";
        expected += "         LDA   $1000,Y\n";
        expected += "         LDA   ($00,X)\n";
        expected += "         LDA   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn ora() {
        let hex = "0900050015000d00101d001019001001001100";
        let mut expected = String::new();
        expected += "         ORA   #$00\n";
        expected += "         ORA   $00\n";
        expected += "         ORA   $00,X\n";
        expected += "         ORA   $1000\n";
        expected += "         ORA   $1000,X\n";
        expected += "         ORA   $1000,Y\n";
        expected += "         ORA   ($00,X)\n";
        expected += "         ORA   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn sbc() {
        let hex = "e900e500f500ed0010fd0010f90010e100f100";
        let mut expected = String::new();
        expected += "         SBC   #$00\n";
        expected += "         SBC   $00\n";
        expected += "         SBC   $00,X\n";
        expected += "         SBC   $1000\n";
        expected += "         SBC   $1000,X\n";
        expected += "         SBC   $1000,Y\n";
        expected += "         SBC   ($00,X)\n";
        expected += "         SBC   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
}

mod store_ops {
    #[test]
    fn sta() {
        let hex = "850095008d00109d001099001081009100";
        let mut expected = String::new();
        expected += "         STA   $00\n";
        expected += "         STA   $00,X\n";
        expected += "         STA   $1000\n";
        expected += "         STA   $1000,X\n";
        expected += "         STA   $1000,Y\n";
        expected += "         STA   ($00,X)\n";
        expected += "         STA   ($00),Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn stx() {
        let hex = "860096008e0010";
        let mut expected = String::new();
        expected += "         STX   $00\n";
        expected += "         STX   $00,Y\n";
        expected += "         STX   $1000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn sty() {
        let hex = "840094008c0010";
        let mut expected = String::new();
        expected += "         STY   $00\n";
        expected += "         STY   $00,X\n";
        expected += "         STY   $1000\n";
        super::test_disassembler(hex, &expected, true);
    }
}

mod index_ops {    
    #[test]
    fn cpx() {
        let hex = "e000e400ec0010";
        let mut expected = String::new();
        expected += "         CPX   #$00\n";
        expected += "         CPX   $00\n";
        expected += "         CPX   $1000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn cpy() {
        let hex = "c000c400cc0010";
        let mut expected = String::new();
        expected += "         CPY   #$00\n";
        expected += "         CPY   $00\n";
        expected += "         CPY   $1000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn ldx() {
        let hex = "a200a600b600ae0010be0010";
        let mut expected = String::new();
        expected += "         LDX   #$00\n";
        expected += "         LDX   $00\n";
        expected += "         LDX   $00,Y\n";
        expected += "         LDX   $1000\n";
        expected += "         LDX   $1000,Y\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn ldy() {
        let hex = "a000a400b400ac0010bc0010";
        let mut expected = String::new();
        expected += "         LDY   #$00\n";
        expected += "         LDY   $00\n";
        expected += "         LDY   $00,X\n";
        expected += "         LDY   $1000\n";
        expected += "         LDY   $1000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
}

mod branching {    
    #[test]
    fn forward_branch() {
        let hex = "907fb010f0003000d000100050007000";
        let mut expected = String::new();
        expected += "         BCC   $0081\n";
        expected += "         BCS   $0014\n";
        expected += "         BEQ   $0006\n";
        expected += "         BMI   $0008\n";
        expected += "         BNE   $000A\n";
        expected += "         BPL   $000C\n";
        expected += "         BVC   $000E\n";
        expected += "         BVS   $0010\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn reverse_branch() {
        let hex = "9000b0fcf0fc30fcd0fc10fc50fc70fc";
        let mut expected = String::new();
        expected += "         BCC   $0002\n";
        expected += "         BCS   $0000\n";
        expected += "         BEQ   $0002\n";
        expected += "         BMI   $0004\n";
        expected += "         BNE   $0006\n";
        expected += "         BPL   $0008\n";
        expected += "         BVC   $000A\n";
        expected += "         BVS   $000C\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn jumping() {
        let hex = "4c00106c00102000104060";
        let mut expected = String::new();
        expected += "         JMP   $1000\n";
        expected += "         JMP   ($1000)\n";
        expected += "         JSR   $1000\n";
        expected += "         RTI\n";
        expected += "         RTS\n";
        super::test_disassembler(hex, &expected, true);
    }
}

mod short_ops {    
    #[test]
    fn status() {
        let hex = "18d858b838f878";
        let mut expected = String::new();
        expected += "         CLC\n";
        expected += "         CLD\n";
        expected += "         CLI\n";
        expected += "         CLV\n";
        expected += "         SEC\n";
        expected += "         SED\n";
        expected += "         SEI\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn stack() {
        let hex = "48086828";
        let mut expected = String::new();
        expected += "         PHA\n";
        expected += "         PHP\n";
        expected += "         PLA\n";
        expected += "         PLP\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn transfer() {
        let hex = "aaa8ba8a9a98";
        let mut expected = String::new();
        expected += "         TAX\n";
        expected += "         TAY\n";
        expected += "         TSX\n";
        expected += "         TXA\n";
        expected += "         TXS\n";
        expected += "         TYA\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn increment() {
        let hex = "ca88e600f600ee0010fe0010e8c8";
        let mut expected = String::new();
        expected += "         DEX\n";
        expected += "         DEY\n";
        expected += "         INC   $00\n";
        expected += "         INC   $00,X\n";
        expected += "         INC   $1000\n";
        expected += "         INC   $1000,X\n";
        expected += "         INX\n";
        expected += "         INY\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn brk() {
        let hex = "0000EA";
        let mut expected = String::new();
        expected += "         BRK   #$00\n";
        expected += "         NOP\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn no_brk() {
        let hex = "0000EA";
        let mut expected = String::new();
        expected += "         DS    2,$00\n";
        expected += "         NOP\n";
        super::test_disassembler(hex, &expected, false);
    }
}

mod bitwise {    
    #[test]
    fn asl() {
        let hex = "0a060016000e00101e0010";
        let mut expected = String::new();
        expected += "         ASL\n";
        expected += "         ASL   $00\n";
        expected += "         ASL   $00,X\n";
        expected += "         ASL   $1000\n";
        expected += "         ASL   $1000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn bit() {
        let hex = "24002c0010";
        let mut expected = String::new();
        expected += "         BIT   $00\n";
        expected += "         BIT   $1000\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn lsr() {
        let hex = "4a460056004e00105e0010";
        let mut expected = String::new();
        expected += "         LSR\n";
        expected += "         LSR   $00\n";
        expected += "         LSR   $00,X\n";
        expected += "         LSR   $1000\n";
        expected += "         LSR   $1000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn rol() {
        let hex = "2a260036002e00103e0010";
        let mut expected = String::new();
        expected += "         ROL\n";
        expected += "         ROL   $00\n";
        expected += "         ROL   $00,X\n";
        expected += "         ROL   $1000\n";
        expected += "         ROL   $1000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
    #[test]
    fn ror() {
        let hex = "6a660076006e00107e0010";
        let mut expected = String::new();
        expected += "         ROR\n";
        expected += "         ROR   $00\n";
        expected += "         ROR   $00,X\n";
        expected += "         ROR   $1000\n";
        expected += "         ROR   $1000,X\n";
        super::test_disassembler(hex, &expected, true);
    }
}