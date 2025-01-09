use super::super::disassembly::{DasmRange, Disassembler};
use super::super::ProcessorType;

fn test_disassembler(hex: &str, expected: &str) {
    let img = hex::decode(hex).expect("hex error");
    let mut disassembler = Disassembler::new();
    let actual = disassembler
        .disassemble(&img, DasmRange::All, ProcessorType::_65c02, "none")
        .expect("dasm error");
    assert_eq!(actual, expected);
}

fn test_disassembler_with_labeling(hex: &str, expected: &str, org: usize) {
    let img = [vec![0;org],hex::decode(hex).expect("hex error")].concat();
    let mut disassembler = Disassembler::new();
    let actual = disassembler
        .disassemble(&img, DasmRange::Range([org,img.len()]), ProcessorType::_65c02, "all")
        .expect("dasm error");
    assert_eq!(actual, expected);
}

mod octet {
    #[test]
    fn adc() {
        let hex = "7200";
        let mut expected = String::new();
        expected += "         ADC   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn and() {
        let hex = "3200";
        let mut expected = String::new();
        expected += "         AND   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn cmp() {
        let hex = "d200";
        let mut expected = String::new();
        expected += "         CMP   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn eor() {
        let hex = "5200";
        let mut expected = String::new();
        expected += "         EOR   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn lda() {
        let hex = "b200";
        let mut expected = String::new();
        expected += "         LDA   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn ora() {
        let hex = "1200";
        let mut expected = String::new();
        expected += "         ORA   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn sbc() {
        let hex = "f200";
        let mut expected = String::new();
        expected += "         SBC   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
}

mod store {
    #[test]
    fn sta() {
        let hex = "9200";
        let mut expected = String::new();
        expected += "         STA   ($00)\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn stz() {
        let hex = "640074009c00109e0010";
        let mut expected = String::new();
        expected += "         STZ   $00\n";
        expected += "         STZ   $00,X\n";
        expected += "         STZ   $1000\n";
        expected += "         STZ   $1000,X\n";
        super::test_disassembler(hex, &expected);
    }
}

mod branching {
    #[test]
    fn relative() {
        let hex = "8000";
        let mut expected = String::new();
        expected += "         BRA   $0002\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn jumping() {
        let hex = "7c0010";
        let mut expected = String::new();
        expected += "         JMP   ($1000,X)\n";
        super::test_disassembler(hex, &expected);
    }
}

mod short {
    #[test]
    fn stack() {
        let hex = "5a7adafa";
        let mut expected = String::new();
        expected += "         PHY\n";
        expected += "         PLY\n";
        expected += "         PHX\n";
        expected += "         PLX\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn increment() {
        let hex = "1a3a";
        let mut expected = String::new();
        expected += "         INC\n";
        expected += "         DEC\n";
        super::test_disassembler(hex, &expected);
    }
}

mod bitwise {
    #[test]
    fn bit() {
        let hex = "340089003c0010";
        let mut expected = String::new();
        expected += "         BIT   $00,X\n";
        expected += "         BIT   #$00\n";
        expected += "         BIT   $1000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn tsb_trb() {
        let hex = "040014000c00101c0010";
        let mut expected = String::new();
        expected += "         TSB   $00\n";
        expected += "         TRB   $00\n";
        expected += "         TSB   $1000\n";
        expected += "         TRB   $1000\n";
        super::test_disassembler(hex, &expected);
    }
}

mod label_substitution {
    #[test]
    fn bit() {
        let hex = "340089003c0003";
        let mut expected = String::new();
        expected += "_0300    BIT   $00,X\n";
        expected += "_0302    BIT   #$00\n";
        expected += "_0304    BIT   _0300,X\n";
        super::test_disassembler_with_labeling(hex, &expected, 0x300);
    }
    #[test]
    fn stz() {
        let hex = "640074009c00109e0203";
        let mut expected = String::new();
        expected += "_0300    STZ   $00\n";
        expected += "_0302    STZ   $00,X\n";
        expected += "_0304    STZ   $1000\n";
        expected += "_0307    STZ   _0302,X\n";
        super::test_disassembler_with_labeling(hex, &expected, 0x300);
    }
}