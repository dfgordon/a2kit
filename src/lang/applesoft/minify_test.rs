use super::minifier;


fn test_minify(test_code: &str,expected: &str,level: usize) {
	let mut minifier = minifier::Minifier::new();
	minifier.set_level(level);
	let actual = minifier.minify(test_code).expect("minify failed");
	assert_eq!(actual,String::from(expected)+"\n");
}

mod minify_vars {
    #[test]
	fn lower_case_long_var() {
		let test_code = "10 HOME\r\n20 PRINT hello";
		let expected = "10HOME\n20PRINThe";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_variable() {
		let test_code = "10 HOME\n20 PRINT HELLO";
		let expected = "10HOME\n20PRINTHE";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn lower_case_long_string() {
		let test_code = "10 HOME\n20 PRINT hello$";
		let expected = "10HOME\n20PRINThe$";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_string() {
		let test_code = "10 HOME\n20 PRINT HELLO$";
		let expected = "10HOME\n20PRINTHE$";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn lower_case_long_int() {
		let test_code = "10 HOME\n20 PRINT hello%";
		let expected = "10HOME\n20PRINThe%";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_int() {
		let test_code = "10 HOME\n20 PRINT HELLO%";
		let expected = "10HOME\n20PRINTHE%";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn lower_case_long_array_name() {
		let test_code = "10 print aero(xa1b2,ya2b1)";
		let expected = "10printae(xa,ya)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_array_name() {
		let test_code = "10 PRINT AERO(XA1B2,YA2B1)";
		let expected = "10PRINTAE(XA,YA)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn lower_case_long_string_array() {
		let test_code = "10 print aero$(xa1b2,ya2b1)";
		let expected = "10printae$(xa,ya)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_string_array() {
		let test_code = "10 PRINT AERO$(XA1B2,YA2B1)";
		let expected = "10PRINTAE$(XA,YA)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn lower_case_long_int_array() {
		let test_code = "10 print aero%(xa1b2,ya2b1)";
		let expected = "10printae%(xa,ya)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_int_array() {
		let test_code = "10 PRINT AERO%(XA1B2,YA2B1)";
		let expected = "10PRINTAE%(XA,YA)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn short_variables_only () {
		let test_code = "10 PRINT A%(X,Y) A$(X%,Y%)";
		let expected = "10PRINTA%(X,Y)A$(X%,Y%)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn amp_func_vars() {
		let test_code = "10 & MYFUNC (HELLO+Y,AERO%(XA1B2,YA2B1),aero$(x1ab2,y1ab1))";
		let expected = "10& MYFUNC (HELLO+Y,AERO%(XA1B2,YA2B1),aero$(x1ab2,y1ab1))";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn amp_expr_list() {
		let test_code = "10 & (\"cmd\",HELLO+Y,AERO%(XA1B2,YA2B1),aero$(x1ab2,y1ab1))";
		let expected = "10&(\"cmd\",HE+Y,AE%(XA,YA),ae$(x1,y1))";
		super::test_minify(test_code ,expected, 3);
	}
    #[test]
	fn amp_overloaded_toks() {
		let test_code = "10 & draw \"subcmd\" at HELLO+Y,AERO%(XA1B2,YA2B1) and aero%(x1ab2,y1ab1)";
		let expected = "10& draw \"subcmd\" at HELLO+Y,AERO%(XA1B2,YA2B1) and aero%(x1ab2,y1ab1)";
		super::test_minify(test_code, expected, 1);
	}
}

mod minify_vars_with_guards {
    #[test]
	fn to_and_step_guards() {
		let test_code = "10 for x = ca12345 t o abracadabra step 5";
		let expected = "10forx=(ca)to(ab)step5";
		// TODO: could save 2 bytes by minifying as `10forx=cato(ab)step5`
		// (Apple tokenizer will resolve ATO correctly)
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn atn_guard() {
		let test_code = "10 hlin x,xrght at n";
		let expected = "10hlinx,xrat n";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn ato_guard() {
		let test_code = "10 draw br at o1,o2";
		let expected = "10drawbrat o1,o2";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn logic_guards() {
		let test_code = "10 if hf123 or it123 and nobody then 100";
		let expected = "10if(hf)or(it)and(no)then100";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn logic_non_guards() {
		let test_code = "10 if hf123% or it123% and nobody% then 100";
		let expected = "10ifhf%orit%andno%then100";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn negated_logic_guards() {
		let test_code = "10 if not hf123 or not it123 and not nobody then 100";
		let expected = "10ifnot(hf)ornot(it)andnot(no)then100";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn not_worth_shortening() {
		let test_code = "10 if not hf12 or not it12 and not nobo then 100";
		let expected = "10ifnothf12ornotit12andnotnobothen100";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn spaces_thrown_in() {
		let test_code = "10 for x = ca1  23 45 to abrac adabra step 5";
		let expected = "10forx=(ca)to(ab)step5";
		super::test_minify(test_code, expected, 1);
	}
}

mod minify_funcs {
    #[test]
	fn lower_case_long_function() {
		let test_code = "10 DEF FN abcd(x12) = x12^2\n20 PRINT FN abcd(x12)";
		let expected = "10DEFFNab(x1)=x1^2\n20PRINTFNab(x1)";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn upper_case_long_function() {
		let test_code = "10 DEF FN ABCD(X12) = X12^2\n20 PRINT FN ABCD(X12)";
		let expected = "10DEFFNAB(X1)=X1^2\n20PRINTFNAB(X1)";
		super::test_minify(test_code, expected, 1);
	}
}

mod minify_separators {
    #[test]
	fn unnecessary_unquote() {
		let test_code = "10 HOME\n20 PRINT \"HELLO\"";
		let expected = "10HOME\n20PRINT\"HELLO";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn unnecessary_unquote_sexpr() {
		let test_code = "10 HOME\n20 A$ = A$ + B$ + \"HELLO\"";
		let expected = "10HOME\n20A$=A$+B$+\"HELLO";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn print_with_nulls() {
		let test_code = "10 print a,b, ,c;d$;;;e$";
		let expected = "10printa,b,,c;d$e$";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn trailing_colon() {
		let test_code = "10 goto 10:";
		let expected = "10goto10";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn extra_colons() {
		let test_code = "10 goto 10::goto 20:::goto 30::::";
		let expected = "10goto10:goto20:goto30";
		super::test_minify(test_code, expected, 1);
	}
    #[test]
	fn trailing_colon_after_unquote() {
		let test_code = "10 print \"1\": print \"2\":";
		let expected = "10print\"1\":print\"2";
		super::test_minify(test_code, expected, 1);
	}
}

mod minify_rem {
	#[test]
	fn rem_trails() {
		let test_code = "10 print x: rem print a number";
		let expected = "10printx";
		super::test_minify(test_code, expected, 1);
	}
	#[test]
	fn rem_no_ref() {
		let test_code = "10 rem print a number\n20 print x";
		let expected = "20printx";
		super::test_minify(test_code, expected, 2);
	}
	#[test]
	fn rem_with_refs() {
		let test_code = "10 rem 1\n11 rem 2\n20 x = x + 1\n30 goto 10: goto 11\n40 rem 3\n50 print x\n60 goto 40\n70 end";
		let expected = "20x=x+1\n30goto20:goto20\n50printx\n60goto50\n70end";
		super::test_minify(test_code, expected, 2);
	}
}

mod combine {
	#[test]
	fn unterminated_str() {
		let test_code = "10 print \"hello\n20 end";
		let expected = "10print\"hello\":end";
		super::test_minify(test_code, expected, 3);
	}
	#[test]
	fn conditional_guards() {
		let test_code = "10 print\n20 if x then print\n30 end";
		let expected = "10print:ifxthenprint\n30end";
		super::test_minify(test_code, expected, 3);
	}
	#[test]
	fn ref_guards() {
		let test_code = "10 print\n20 print\n30 print\n40 goto 30:end";
		let expected = "10print:print\n30print:goto30:end";
		super::test_minify(test_code, expected, 3);
	}
	#[test]
	fn all_forbidden() {
		let test_code = "10 del 20,30\n20 print\n30 end";
		let expected = "10del20,30\n20print\n30end";
		super::test_minify(test_code, expected, 3);
	}
	#[test]
	fn with_deletion() {
		let test_code = "10 rem testing\n20 print\n30 print\n40 input a$\n50 on x goto 10,40";
		let expected = "20print:print\n40inputa$:onxgoto20,40";
		super::test_minify(test_code, expected, 3);
	}
}