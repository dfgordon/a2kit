use tree_sitter;

fn test_assign_expr(text: &str,expected: i64) {
    let symbols = super::super::Symbols::new();
	let mut parser = tree_sitter::Parser::new();
	parser.set_language(&tree_sitter_merlin6502::LANGUAGE.into()).expect("failed to init");
    let tree = parser.parse(text,None).unwrap();
    let mut curs = tree.walk();
    curs.goto_first_child(); // pseudo_operation
    curs.goto_last_child(); // arg_equ
    assert_eq!(curs.node().kind(),"arg_equ");
    let actual = super::super::assembly::eval_expr(&curs.node(), text,None, &symbols,None).expect("expression failed");
    assert_eq!(actual,expected);
}

#[test]
fn unary() {
    test_assign_expr("x = -1\n",-1);
    test_assign_expr("x = +1\n",1);
    test_assign_expr("x = -1+2\n",1);
    test_assign_expr("x = -{1+2}\n",-3);
}

#[test]
fn simple_binary() {
    test_assign_expr("x = 1+2\n",3);
    test_assign_expr("x = 1-2\n",-1);
    test_assign_expr("x = 5*9\n",45);
    test_assign_expr("x = 1/4\n",0);
    test_assign_expr("x = 5/4\n",1);
}

#[test]
fn left_right_prec() {
    test_assign_expr("x = 1+2*2\n",6);
    test_assign_expr("x = 1-2*2\n",-2);
    test_assign_expr("x = 5+2/5\n",1);
}

#[test]
fn braced_prec() {
    test_assign_expr("x = {1+2*2}\n",5);
    test_assign_expr("x = {1-2*2}\n",-3);
    test_assign_expr("x = {5+2/5}\n",5);
}

#[test]
fn bitwise() {
    test_assign_expr("x = %1010.%1100", 0b1110);
    test_assign_expr("x = %1010&%1100", 0b1000);
    test_assign_expr("x = %1010!%1110", 0b0100);
}

#[test]
fn comparison() {
    test_assign_expr("x = 1<2", 1);
    test_assign_expr("x = 1>2", 0);
    test_assign_expr("x = 1=2", 0);
    test_assign_expr("x = 1#2", 1);
}
