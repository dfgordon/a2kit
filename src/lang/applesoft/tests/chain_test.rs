use tree_sitter;

fn test_chain_statement(text: &str,expected_prog: &str,expected_line: isize) {
	let mut parser = tree_sitter::Parser::new();
	parser.set_language(&tree_sitter_applesoft::LANGUAGE.into()).expect("failed to init");
    let tree = parser.parse(text,None).unwrap();
    let mut curs = tree.walk();
    curs.goto_first_child(); // line
    curs.goto_first_child(); // linenum
    curs.goto_next_sibling(); // statement
    curs.goto_first_child(); // tok_print
    let (actual_prog,actual_line) = super::super::diagnostics::chain::test_chain(&curs.node(), text).unwrap();
    assert_eq!(actual_prog,expected_prog);
    assert_eq!(actual_line,expected_line);
}

#[test]
fn dos33() {
    test_chain_statement("10 CALL 520\"STAGE 2 PROG\"\n","STAGE 2 PROG",-1);
}

#[test]
fn dstring() {
    test_chain_statement("10 print d$;\"chain hello\"\n","hello",-1);
}

#[test]
fn direct_ctrl() {
    test_chain_statement("10 PRINT \"\u{0004}CHAIN PROG\"\n","PROG",-1);
}

#[test]
fn dstring_spaces() {
    test_chain_statement("10 PRINT D$;\"C HA IN PROG\"\n","PROG",-1);
}

#[test]
fn direct_ctrl_spaces() {
    test_chain_statement("10 PRINT \"\u{0004} C HA   INPROG\"\n","PROG",-1);
}

#[test]
fn dstring_intervening_path() {
    test_chain_statement("10 PRINT D$;\"CHAIN \";PTH$;\"PROG\"\n","PROG",-1);
}

#[test]
fn dstring_intervening_path_linenum() {
    test_chain_statement("10 PRINT D$;\"CHAIN\";PTH$;\"PROG,@392\"\n","PROG",392);
}

#[test]
fn chr4_intervening_path() {
    test_chain_statement("10 PRINT CHR$ (4) ; \"CHAIN \";PTH$;\"PROG\"\n","PROG",-1);
}

#[test]
fn chr4_intervening_path_linenum() {
    test_chain_statement("10 PRINT CHR$ (4) ; \"CHAIN\";PTH$;\"PROG,@392\"\n","PROG",392);
}

