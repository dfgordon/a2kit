use tree_sitter;

fn test_assign_expr(text: &str,expected: i64) {
    let symbols = super::super::Symbols::new();
	let mut parser = tree_sitter::Parser::new();
	parser.set_language(&tree_sitter_integerbasic::LANGUAGE.into()).expect("failed to init");
    let tree = parser.parse(text,None).unwrap();
    let mut curs = tree.walk();
    curs.goto_first_child(); // line
    curs.goto_first_child(); // linenum
    curs.goto_next_sibling(); // statement
    curs.goto_first_child(); // assignment
    curs.goto_last_child(); // expression
    let actual = super::super::diagnostics::expression::eval_aexpr(&curs.node(), text, &symbols).expect("expression failed");
    assert_eq!(actual,expected);
}

#[test]
fn unary() {
    test_assign_expr("10 x = -1\n",-1);
    test_assign_expr("10 x = -1 + 2\n",1);
    test_assign_expr("10 x = -(1+2)\n",-3);
}

#[test]
fn simple_binary() {
    test_assign_expr("10 x = 1 + 2\n",3);
    test_assign_expr("10 x = 1 - 2\n",-1);
    test_assign_expr("10 x = 5 * 9\n",45);
    test_assign_expr("10 x = 1 / 4\n",0);
    test_assign_expr("10 x = 7 / 4\n",1);
}

#[test]
fn simple_prec() {
    test_assign_expr("10 x = 1 + 2 * 2\n",5);
    test_assign_expr("10 x = 1 - 2 * 2\n",-3);
    test_assign_expr("10 x = 5 + 2 / 5\n",5);
}

#[test]
fn logic_vals() {
    test_assign_expr("10 x = 1 > 0\n",1);
    test_assign_expr("10 x = 115 < 1\n",0);
    test_assign_expr("10 x = -5 >= 1\n",0);
    test_assign_expr("10 x = -3 <= 1\n",1);
    test_assign_expr("10 x = -3 <> 1\n",1);
    test_assign_expr("10 x = 1 # 1\n",0);
}

#[test]
fn funcs() {
    test_assign_expr("10 x = 3 * (3 + abs((5-2)*sgn(-5)))", 18);
    test_assign_expr("10 x = 3 * (3 + 93 mod 90)", 18);
}