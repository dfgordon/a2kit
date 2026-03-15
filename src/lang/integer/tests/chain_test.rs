use tree_sitter;

fn test_chain_statement(text: &str,expected: &str) {
	let mut parser = tree_sitter::Parser::new();
	parser.set_language(&tree_sitter_integerbasic::LANGUAGE.into()).expect("failed to init");
    let tree = parser.parse(text,None).unwrap();
    let mut curs = tree.walk();
    curs.goto_first_child(); // line
    curs.goto_first_child(); // linenum
    curs.goto_next_sibling(); // statement
    curs.goto_first_child(); // statement_print_str
    assert_eq!(curs.node().kind(),"statement_print_str");
    let actual = super::super::diagnostics::chain::test_chain(&curs.node(), text).unwrap();
    assert_eq!(actual,expected);
}

#[test]
fn canonical() {
    test_chain_statement("10 print d$;\"chain hello\"\n","hello");
}

#[test]
fn direct_ctrl() {
    test_chain_statement("10 PRINT \"\u{0004}CHAIN PROG\"\n","PROG");
}

#[test]
fn dstring_spaces() {
    test_chain_statement("10 PRINT D$;\"C HA IN PROG\"\n","PROG");
}

#[test]
fn direct_ctrl_spaces() {
    test_chain_statement("10 PRINT \"\u{0004} C HA   INPROG\"\n","PROG");
}

