use super::super::disassembly::{DasmRange, Disassembler};
use super::super::ProcessorType;

fn test_disassembler(hex: &str, expected: &str) {
    let img = hex::decode(hex).expect("hex error");
    let mut disassembler = Disassembler::new();
    disassembler.set_mx(false,false);
    let actual = disassembler
        .disassemble(&img, DasmRange::All, ProcessorType::_65c816, "none")
        .expect("dasm error");
    assert_eq!(actual, expected);
}

fn test_disassembler_with_labeling(hex: &str, expected: &str, org: usize) {
    let img = [vec![0;org],hex::decode(hex).expect("hex error")].concat();
    let mut disassembler = Disassembler::new();
    let actual = disassembler
        .disassemble(&img, DasmRange::Range([org,img.len()]), ProcessorType::_65c816, "all")
        .expect("dasm error");
    assert_eq!(actual, expected);
}

mod forced_long_suffix {
    #[test]
    fn adc() {
        let hex = "6f0000007f000000";
        let mut expected = String::new();
        expected += "         ADCL  $000000\n";
        expected += "         ADCL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn and() {
        let hex = "2f0000003f000000";
        let mut expected = String::new();
        expected += "         ANDL  $000000\n";
        expected += "         ANDL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn eor() {
        let hex = "4f0000005f000000";
        let mut expected = String::new();
        expected += "         EORL  $000000\n";
        expected += "         EORL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn lda() {
        let hex = "af000000bf000000";
        let mut expected = String::new();
        expected += "         LDAL  $000000\n";
        expected += "         LDAL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn ora() {
        let hex = "0f0000001f000000";
        let mut expected = String::new();
        expected += "         ORAL  $000000\n";
        expected += "         ORAL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn sta() {
        let hex = "8f0000009f000000";
        let mut expected = String::new();
        expected += "         STAL  $000000\n";
        expected += "         STAL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn cmp() {
        let hex = "cf000000df000000";
        let mut expected = String::new();
        expected += "         CMPL  $000000\n";
        expected += "         CMPL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn sbc() {
        let hex = "ef000000ff000000";
        let mut expected = String::new();
        expected += "         SBCL  $000000\n";
        expected += "         SBCL  $000000,X\n";
        super::test_disassembler(hex, &expected);
    }
}

// mod forced_long_prefix {
//     #[test]
//     fn adc() {
//         let hex = "6f0000007f000000";
//         let mut expected = String::new();
//         expected += "         ADC   >$000000\n";
//         expected += "         ADC   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn and() {
//         let hex = "2f0000003f000000";
//         let mut expected = String::new();
//         expected += "         AND   >$000000\n";
//         expected += "         AND   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn eor() {
//         let hex = "4f0000005f000000";
//         let mut expected = String::new();
//         expected += "         EOR   >$000000\n";
//         expected += "         EOR   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn lda() {
//         let hex = "af000000bf000000";
//         let mut expected = String::new();
//         expected += "         LDA   >$000000\n";
//         expected += "         LDA   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn ora() {
//         let hex = "0f0000001f000000";
//         let mut expected = String::new();
//         expected += "         ORA   >$000000\n";
//         expected += "         ORA   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn sta() {
//         let hex = "8f0000009f000000";
//         let mut expected = String::new();
//         expected += "         STA   >$000000\n";
//         expected += "         STA   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn cmp() {
//         let hex = "cf000000df000000";
//         let mut expected = String::new();
//         expected += "         CMP   >$000000\n";
//         expected += "         CMP   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
//     #[test]
//     fn sbc() {
//         let hex = "ef000000ff000000";
//         let mut expected = String::new();
//         expected += "         SBC   >$000000\n";
//         expected += "         SBC   >$000000,X\n";
//         super::test_disassembler(hex, &expected);
//     }
// }

mod octet {
    #[test]
    fn adc() {
        let hex = "69fe1f6300730067007700";
        let mut expected = String::new();
        expected += "         ADC   #$1FFE\n";
        expected += "         ADC   $00,S\n";
        expected += "         ADC   ($00,S),Y\n";
        expected += "         ADC   [$00]\n";
        expected += "         ADC   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn and() {
        let hex = "2300330027003700";
        let mut expected = String::new();
        expected += "         AND   $00,S\n";
        expected += "         AND   ($00,S),Y\n";
        expected += "         AND   [$00]\n";
        expected += "         AND   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn eor() {
        let hex = "4300530047005700";
        let mut expected = String::new();
        expected += "         EOR   $00,S\n";
        expected += "         EOR   ($00,S),Y\n";
        expected += "         EOR   [$00]\n";
        expected += "         EOR   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn lda() {
        let hex = "a300b300a700b700";
        let mut expected = String::new();
        expected += "         LDA   $00,S\n";
        expected += "         LDA   ($00,S),Y\n";
        expected += "         LDA   [$00]\n";
        expected += "         LDA   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn ora() {
        let hex = "0300130007001700";
        let mut expected = String::new();
        expected += "         ORA   $00,S\n";
        expected += "         ORA   ($00,S),Y\n";
        expected += "         ORA   [$00]\n";
        expected += "         ORA   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn cmp() {
        let hex = "c300d300c700d700";
        let mut expected = String::new();
        expected += "         CMP   $00,S\n";
        expected += "         CMP   ($00,S),Y\n";
        expected += "         CMP   [$00]\n";
        expected += "         CMP   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn sbc() {
        let hex = "e300f300e700f700";
        let mut expected = String::new();
        expected += "         SBC   $00,S\n";
        expected += "         SBC   ($00,S),Y\n";
        expected += "         SBC   [$00]\n";
        expected += "         SBC   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
}

mod store {
    #[test]
    fn sta() {
        let hex = "8300930087009700";
        let mut expected = String::new();
        expected += "         STA   $00,S\n";
        expected += "         STA   ($00,S),Y\n";
        expected += "         STA   [$00]\n";
        expected += "         STA   [$00],Y\n";
        super::test_disassembler(hex, &expected);
    }
}

mod branching {
    #[test]
    fn forward_branch() {
        let hex = "82ff7f";
        let mut expected = String::new();
        expected += "         BRL   $8002\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn reverse_branch() {
        let hex = "82000082faff";
        let mut expected = String::new();
        expected += "         BRL   $0003\n";
        expected += "         BRL   $0000\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn jumping() {
        let hex = "fc00105c00100322001003dc00106b";
        let mut expected = String::new();
        expected += "         JSR   ($1000,X)\n";
        expected += "         JML   $031000\n";
        expected += "         JSL   $031000\n";
        expected += "         JML   ($1000)\n";
        expected += "         RTL\n";
        super::test_disassembler(hex, &expected);
    }
}

mod short {
    #[test]
    fn stack() {
        let hex = "0b2b4b8bab";
        let mut expected = String::new();
        expected += "         PHD\n";
        expected += "         PLD\n";
        expected += "         PHK\n";
        expected += "         PHB\n";
        expected += "         PLB\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn transfer() {
        let hex = "1b3b5b7b9bbb";
        let mut expected = String::new();
        expected += "         TCS\n";
        expected += "         TSC\n";
        expected += "         TCD\n";
        expected += "         TDC\n";
        expected += "         TXY\n";
        expected += "         TYX\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn exchange() {
        let hex = "ebfb";
        let mut expected = String::new();
        expected += "         XBA\n";
        expected += "         XCE\n";
        super::test_disassembler(hex, &expected);
    }
}

mod bitwise {
    #[test]
    fn status_bits() {
        let hex = "c2fee201";
        let mut expected = String::new();
        expected += "         REP   $FE\n";
        expected += "         SEP   $01\n";
        super::test_disassembler(hex, &expected);
    }
}

mod other {
    #[test]
    fn cop() {
        let hex = "0200";
        let mut expected = String::new();
        expected += "         COP   $00\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn mem_move() {
        let hex = "440100540100";
        let mut expected = String::new();
        expected += "         MVP   $00,$01\n";
        expected += "         MVN   $00,$01\n";
        super::test_disassembler(hex, &expected);
    }
    #[test]
    fn stack() {
        let hex = "62fd7fd406f40081";
        let mut expected = String::new();
        expected += "         PER   $8000\n";
        expected += "         PEI   ($06)\n";
        expected += "         PEA   $8100\n";
        super::test_disassembler(hex, &expected);
    }
}

mod label_substitution {
    #[test]
    fn stack() {
        let hex = "620200d406f40380";
        let mut expected = String::new();
        expected += "_8000    PER   _8005\n";
        expected += "_8003    PEI   ($06)\n";
        expected += "_8005    PEA   _8003\n";
        super::test_disassembler_with_labeling(hex, &expected, 0x8000);
    }
    #[test]
    fn jumping() {
        let hex = "fc00105c00100322001003dc03806b";
        let mut expected = String::new();
        expected += "_8000    JSR   ($1000,X)\n";
        expected += "_8003    JML   $031000\n";
        expected += "_8007    JSL   $031000\n";
        expected += "_800B    JML   (_8003)\n";
        expected += "_800E    RTL\n";
        super::test_disassembler_with_labeling(hex, &expected, 0x8000);
    }
    #[test]
    fn lda() {
        let hex = "a380b386a782b784";
        let mut expected = String::new();
        expected += "_0080    LDA   _0080,S\n";
        expected += "_0082    LDA   (_0086,S),Y\n";
        expected += "_0084    LDA   [_0082]\n";
        expected += "_0086    LDA   [_0084],Y\n";
        super::test_disassembler_with_labeling(hex, &expected, 0x80);
    }
}