#[cfg(test)]
use super::minifier::Minifier;


#[cfg(test)]
fn test_minify(test_code: &str,expected: &str) {
	let mut minifier = Minifier::new();
	let actual = minifier.minify(test_code);
	assert_eq!(actual,String::from(expected)+"\n");
}

mod minify_vars {
    #[test]
	fn lower_case_long_var() {
		let test_code = "10 HOME\n20 PRINT hello";
		let expected = "10HOME\n20PRINThe";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_variable() {
		let test_code = "10 HOME\n20 PRINT HELLO";
		let expected = "10HOME\n20PRINTHE";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn lower_case_long_string() {
		let test_code = "10 HOME\n20 PRINT hello$";
		let expected = "10HOME\n20PRINThe$";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_string() {
		let test_code = "10 HOME\n20 PRINT HELLO$";
		let expected = "10HOME\n20PRINTHE$";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn lower_case_long_int() {
		let test_code = "10 HOME\n20 PRINT hello%";
		let expected = "10HOME\n20PRINThe%";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_int() {
		let test_code = "10 HOME\n20 PRINT HELLO%";
		let expected = "10HOME\n20PRINTHE%";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn lower_case_long_array_name() {
		let test_code = "10 print aero(xa1b2,ya2b1)";
		let expected = "10printae(xa,ya)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_array_name() {
		let test_code = "10 PRINT AERO(XA1B2,YA2B1)";
		let expected = "10PRINTAE(XA,YA)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn lower_case_long_string_array() {
		let test_code = "10 print aero$(xa1b2,ya2b1)";
		let expected = "10printae$(xa,ya)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_string_array() {
		let test_code = "10 PRINT AERO$(XA1B2,YA2B1)";
		let expected = "10PRINTAE$(XA,YA)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn lower_case_long_int_array() {
		let test_code = "10 print aero%(xa1b2,ya2b1)";
		let expected = "10printae%(xa,ya)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_int_array() {
		let test_code = "10 PRINT AERO%(XA1B2,YA2B1)";
		let expected = "10PRINTAE%(XA,YA)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn short_variables_only () {
		let test_code = "10 PRINT A%(X,Y) A$(X%,Y%)";
		let expected = "10PRINTA%(X,Y)A$(X%,Y%)";
		super::test_minify(test_code, expected);
	}
}

mod minify_vars_with_guards {
    #[test]
	fn to_and_step_guards() {
		let test_code = "10 for x = ca12345 to abracadabra step 5";
		let expected = "10forx=(ca)to(ab)step5";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn logic_guards() {
		let test_code = "10 if hf123 or it123 and nobody then 100";
		let expected = "10if(hf)or(it)and(no)then100";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn logic_non_guards() {
		let test_code = "10 if hf123% or it123% and nobody% then 100";
		let expected = "10ifhf%orit%andno%then100";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn negated_logic_guards() {
		let test_code = "10 if not hf123 or not it123 and not nobody then 100";
		let expected = "10ifnot(hf)ornot(it)andnot(no)then100";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn not_worth_shortening() {
		let test_code = "10 if not hf12 or not it12 and not nobo then 100";
		let expected = "10ifnothf12ornotit12andnotnobothen100";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn spaces_thrown_in() {
		let test_code = "10 for x = ca1  23 45 to abrac adabra step 5";
		let expected = "10forx=(ca)to(ab)step5";
		super::test_minify(test_code, expected);
	}
}

mod minify_funcs {
    #[test]
	fn lower_case_long_function() {
		let test_code = "10 DEF FN abcd(x12) = x12^2\n20 PRINT FN abcd(x12)";
		let expected = "10DEFFNab(x1)=x1^2\n20PRINTFNab(x1)";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn upper_case_long_function() {
		let test_code = "10 DEF FN ABCD(X12) = X12^2\n20 PRINT FN ABCD(X12)";
		let expected = "10DEFFNAB(X1)=X1^2\n20PRINTFNAB(X1)";
		super::test_minify(test_code, expected);
	}
}

mod minify_separators {
    #[test]
	fn unnecessary_unquote() {
		let test_code = "10 HOME\n20 PRINT \"HELLO\"";
		let expected = "10HOME\n20PRINT\"HELLO";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn print_with_nulls() {
		let test_code = "10 print a,b, ,c;d$;;;e$";
		let expected = "10printa,b,,c;d$;;;e$";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn trailing_colon() {
		let test_code = "10 goto 10:";
		let expected = "10goto10";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn extra_colons() {
		let test_code = "10 goto 10::goto 20:::goto 30::::";
		let expected = "10goto10:goto20:goto30";
		super::test_minify(test_code, expected);
	}
    #[test]
	fn trailing_colon_after_unquote() {
		let test_code = "10 print \"1\": print \"2\":";
		let expected = "10print\"1\":print\"2";
		super::test_minify(test_code, expected);
	}
}
