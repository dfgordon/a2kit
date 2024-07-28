use super::super::assembly::Assembler;
use super::super::ProcessorType;
use super::super::Symbols;

fn test_assembler(hex: &str, code: String, pc: usize) {
    let line_count = code.lines().count();
    let img = hex::decode(hex).expect("hex error");
    let mut assembler = Assembler::new();
    let mut symbols = Symbols::new();
    symbols.processor = ProcessorType::_65c02;
    assembler.use_shared_symbols(std::sync::Arc::new(symbols));
    let actual = assembler
        .spot_assemble(code, 0, line_count as isize, Some(pc))
        .expect("asm error");
    assert_eq!(actual, img);
}

mod octet {
    #[test]
    fn adc() {
        let hex = "7200";
        let mut test_code = String::new();
        test_code += "         ADC   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn and() {
        let hex = "3200";
        let mut test_code = String::new();
        test_code += "         AND   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn cmp() {
        let hex = "d200";
        let mut test_code = String::new();
        test_code += "         CMP   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn eor() {
        let hex = "5200";
        let mut test_code = String::new();
        test_code += "         EOR   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn lda() {
        let hex = "b200";
        let mut test_code = String::new();
        test_code += "         LDA   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn ora() {
        let hex = "1200";
        let mut test_code = String::new();
        test_code += "         ORA   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn sbc() {
        let hex = "f200";
        let mut test_code = String::new();
        test_code += "         SBC   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod store {
    #[test]
    fn sta() {
        let hex = "9200";
        let mut test_code = String::new();
        test_code += "         STA   ($00)\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn stz() {
        let hex = "640074009c00109e0010";
        let mut test_code = String::new();
        test_code += "         STZ   $00\n";
        test_code += "         STZ   $00,X\n";
        test_code += "         STZ   $1000\n";
        test_code += "         STZ   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod branching {
    #[test]
    fn relative() {
        let hex = "8000";
        let mut test_code = String::new();
        test_code += "         BRA   $0002\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn jumping() {
        let hex = "7c0010";
        let mut test_code = String::new();
        test_code += "         JMP   ($1000,X)\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod short {
    #[test]
    fn stack() {
        let hex = "5a7adafa";
        let mut test_code = String::new();
        test_code += "         PHY\n";
        test_code += "         PLY\n";
        test_code += "         PHX\n";
        test_code += "         PLX\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn increment() {
        let hex = "1a3a";
        let mut test_code = String::new();
        test_code += "         INC\n";
        test_code += "         DEC\n";
        super::test_assembler(hex, test_code, 0);
    }
}

mod bitwise {
    #[test]
    fn bit() {
        let hex = "340089003c0010";
        let mut test_code = String::new();
        test_code += "         BIT   $00,X\n";
        test_code += "         BIT   #$00\n";
        test_code += "         BIT   $1000,X\n";
        super::test_assembler(hex, test_code, 0);
    }
    #[test]
    fn tsb_trb() {
        let hex = "040014000c00101c0010";
        let mut test_code = String::new();
        test_code += "         TSB   $00\n";
        test_code += "         TRB   $00\n";
        test_code += "         TSB   $1000\n";
        test_code += "         TRB   $1000\n";
        super::test_assembler(hex, test_code, 0);
    }
}
