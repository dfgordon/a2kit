use super::super::assembly::Assembler;
use super::super::{MerlinVersion,ProcessorType};
use super::super::Symbols;

fn test_assembler(hex: &str, code: String, pc: usize) {
    let line_count = code.lines().count();
    let img = hex::decode(hex).expect("hex error");
    let mut assembler = Assembler::new();
    let mut symbols = Symbols::new();
    symbols.processor = ProcessorType::_65c816;
    symbols.assembler = MerlinVersion::Merlin32;
    assembler.use_shared_symbols(std::sync::Arc::new(symbols));
    let actual = assembler
        .spot_assemble(code, 0, line_count as isize, Some(pc))
        .expect("asm error");
    assert_eq!(actual, img);
}

mod forced_long_suffix {
    #[test]
    fn adc() {
        let hex = "6f0000007f000000";
        let mut test_code = String::new();
        test_code += "         ADCL  $000000\n";
        test_code += "         ADCL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn and() {
        let hex = "2f0000003f000000";
        let mut test_code = String::new();
        test_code += "         ANDL  $000000\n";
        test_code += "         ANDL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn eor() {
        let hex = "4f0000005f000000";
        let mut test_code = String::new();
        test_code += "         EORL  $000000\n";
        test_code += "         EORL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn lda() {
        let hex = "af000000bf000000";
        let mut test_code = String::new();
        test_code += "         LDAL  $000000\n";
        test_code += "         LDAL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn ora() {
        let hex = "0f0000001f000000";
        let mut test_code = String::new();
        test_code += "         ORAL  $000000\n";
        test_code += "         ORAL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn sta() {
        let hex = "8f0000009f000000";
        let mut test_code = String::new();
        test_code += "         STAL  $000000\n";
        test_code += "         STAL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn cmp() {
        let hex = "cf000000df000000";
        let mut test_code = String::new();
        test_code += "         CMPL  $000000\n";
        test_code += "         CMPL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn sbc() {
        let hex = "ef000000ff000000";
        let mut test_code = String::new();
        test_code += "         SBCL  $000000\n";
        test_code += "         SBCL  $000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod forced_long_prefix {
    #[test]
    fn adc() {
        let hex = "6f0000007f000000";
        let mut test_code = String::new();
        test_code += "         ADC   >$000000\n";
        test_code += "         ADC   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn and() {
        let hex = "2f0000003f000000";
        let mut test_code = String::new();
        test_code += "         AND   >$000000\n";
        test_code += "         AND   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn eor() {
        let hex = "4f0000005f000000";
        let mut test_code = String::new();
        test_code += "         EOR   >$000000\n";
        test_code += "         EOR   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn lda() {
        let hex = "af000000bf000000";
        let mut test_code = String::new();
        test_code += "         LDA   >$000000\n";
        test_code += "         LDA   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn ora() {
        let hex = "0f0000001f000000";
        let mut test_code = String::new();
        test_code += "         ORA   >$000000\n";
        test_code += "         ORA   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn sta() {
        let hex = "8f0000009f000000";
        let mut test_code = String::new();
        test_code += "         STA   >$000000\n";
        test_code += "         STA   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn cmp() {
        let hex = "cf000000df000000";
        let mut test_code = String::new();
        test_code += "         CMP   >$000000\n";
        test_code += "         CMP   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn sbc() {
        let hex = "ef000000ff000000";
        let mut test_code = String::new();
        test_code += "         SBC   >$000000\n";
        test_code += "         SBC   >$000000,X\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod octet {
    #[test]
    fn adc() {
        let hex = "69fe1f6300730067007700";
        let mut test_code = String::new();
        test_code += "         MX    %00\n";
        test_code += "         ADC   #$1ffe\n";
        test_code += "         ADC   $00,S\n";
        test_code += "         ADC   ($00,S),Y\n";
        test_code += "         ADC   [$00]\n";
        test_code += "         ADC   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn and() {
        let hex = "2300330027003700";
        let mut test_code = String::new();
        test_code += "         AND   $00,S\n";
        test_code += "         AND   ($00,S),Y\n";
        test_code += "         AND   [$00]\n";
        test_code += "         AND   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn eor() {
        let hex = "4300530047005700";
        let mut test_code = String::new();
        test_code += "         EOR   $00,S\n";
        test_code += "         EOR   ($00,S),Y\n";
        test_code += "         EOR   [$00]\n";
        test_code += "         EOR   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn lda() {
        let hex = "a300b300a700b700";
        let mut test_code = String::new();
        test_code += "         LDA   $00,S\n";
        test_code += "         LDA   ($00,S),Y\n";
        test_code += "         LDA   [$00]\n";
        test_code += "         LDA   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn ora() {
        let hex = "0300130007001700";
        let mut test_code = String::new();
        test_code += "         ORA   $00,S\n";
        test_code += "         ORA   ($00,S),Y\n";
        test_code += "         ORA   [$00]\n";
        test_code += "         ORA   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn cmp() {
        let hex = "c300d300c700d700";
        let mut test_code = String::new();
        test_code += "         CMP   $00,S\n";
        test_code += "         CMP   ($00,S),Y\n";
        test_code += "         CMP   [$00]\n";
        test_code += "         CMP   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn sbc() {
        let hex = "e300f300e700f700";
        let mut test_code = String::new();
        test_code += "         SBC   $00,S\n";
        test_code += "         SBC   ($00,S),Y\n";
        test_code += "         SBC   [$00]\n";
        test_code += "         SBC   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod store {
    #[test]
    fn sta() {
        let hex = "8300930087009700";
        let mut test_code = String::new();
        test_code += "         STA   $00,S\n";
        test_code += "         STA   ($00,S),Y\n";
        test_code += "         STA   [$00]\n";
        test_code += "         STA   [$00],Y\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod branching {
    #[test]
    fn forward_branch() {
        let hex = "82fd0f";
        let mut test_code = String::new();
        test_code += "         BRL   $9000\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn reverse_branch() {
        let hex = "82fd8f";
        let mut test_code = String::new();
        test_code += "         BRL   $1000\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn jumping() {
        let hex = "fc00105c00100322001003dc00106b";
        let mut test_code = String::new();
        test_code += "         JSR   ($1000,X)\n";
        test_code += "         JML   $031000\n";
        test_code += "         JSL   $031000\n";
        test_code += "         JML   ($1000)\n";
        test_code += "         RTL\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod short {
    #[test]
    fn stack() {
        let hex = "0b2b4b8bab";
        let mut test_code = String::new();
        test_code += "         PHD\n";
        test_code += "         PLD\n";
        test_code += "         PHK\n";
        test_code += "         PHB\n";
        test_code += "         PLB\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn transfer() {
        let hex = "1b3b5b7b9bbb";
        let mut test_code = String::new();
        test_code += "         TCS\n";
        test_code += "         TSC\n";
        test_code += "         TCD\n";
        test_code += "         TDC\n";
        test_code += "         TXY\n";
        test_code += "         TYX\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn exchange() {
        let hex = "ebfb";
        let mut test_code = String::new();
        test_code += "         XBA\n";
        test_code += "         XCE\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod bitwise {
    #[test]
    fn status_bits() {
        let hex = "c2fee201c2fee201";
        let mut test_code = String::new();
        test_code += "         REP   $FE\n";
        test_code += "         SEP   $01\n";
        test_code += "         REP   #$FE\n";
        test_code += "         SEP   #>$0100\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
}

mod other {
    #[test]
    fn cop() {
        let hex = "0200";
        let mut test_code = String::new();
        test_code += "         COP   $00\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn mem_move() {
        let hex = "440100540100";
        let mut test_code = String::new();
        test_code += "         MVP   $00,$01\n";
        test_code += "         MVN   $00,#^$010203\n";
        super::test_assembler(hex, test_code, 0x8000);
    }
    #[test]
    fn stack() {
        let hex = "62fdffd406f40081f40081f48100";
        let mut test_code = String::new();
        test_code += "         PER   $8000\n";
        test_code += "         PEI   ($06)\n";
        test_code += "         PEA   $8100\n";
        test_code += "         PEA   #>$810000\n";
        test_code += "         PEA   ^$810102\n";
        super::test_assembler(hex, test_code, 0x8000);
    }

}