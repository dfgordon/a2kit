use serde_json;
use std::collections::HashMap;

// The ordering of the inner list matters.
const STATEMENTS_JSON: &str = r#"
{
    "tok_abs": [
        "absolute value",
        "`ABS (aexpr)`"
    ],
    "tok_asc": [
        "ASCII code of first character",
        "`ASC (sexpr)`"
    ],
    "tok_amp": [
        "Execute JMP at $03F5.  The syntax is arbitrary, but in order to be useful, the language server has to impose some limits.",
        "`& [{character}]`",
        {
            "examples": [
                "& \"hello from ampersand\"",
                "& (X/5,A$,\"hello from ampersand\")",
                "& print \"overloaded print\" at x,y"
            ]
        }
    ],
    "tok_atn": [
        "arc tangent in radians",
        "`ATN (aexpr)`"
    ],
    "tok_call": [
        "Call machine language subroutine at decimal address.  The optional arguments can only occur for specialized object code.",
        "`CALL aexpr [string][{,expr}]`",
        {
            "examples": [
                "CALL 768",
                "CALL 520\"NEXT PROGRAM\""
            ]
        }
    ],
    "tok_chr": [
        "character corresponding to ASCII code",
        "`CHR$ (aexpr)`"
    ],
    "tok_clear": [
        "Reset all variables and internal control information",
        "`CLEAR`"
    ],
    "tok_coloreq": [
        "Set the low resolution color",
        "`COLOR = aexpr`"
    ],
    "tok_cont": [
        "Resume program execution, immediate mode only",
        "`CONT`"
    ],
    "tok_cos": [
        "cosine, the argument is in radians",
        "`COS (aexpr)`"
    ],
    "tok_data": [
        "create list of items to be loaded into variables using `READ`",
        "`DATA [literal|string|real|integer][{,[literal|string|real|integer]}]`",
        {
            "examples": [
                "DATA literal 1, \"HELLO\", \"WORLD\", 1.5, -3",
                "READ A$,B$,C$,X,L"
            ]
        }
    ],
    "tok_def": [
        "define a function",
        "`DEF FN name(name) = aexpr`",
        {
            "examples": [
                "DEF FN CUBE(X) = X^3",
                "Y = FN CUBE(3)"
            ]
        }
    ],
    "tok_del": [
        "delete a range of program lines inclusively",
        "`DEL linenum,linenum`"
    ],
    "tok_dim": [
        "allocate space for arrays, specifying the last element (length-1)",
        "`DIM name[%|$] subscript [{,name[%|$] subscript}]`"
    ],
    "tok_draw": [
        "draw a high resolution shape",
        "`DRAW aexpr [AT aexpr,aexpr]`"
    ],
    "tok_end": [
        "stop program execution",
        "`END`"
    ],
    "tok_exp": [
        "exponential",
        "`EXP (aexpr)`"
    ],
    "tok_flash": [
        "switch to flashing text, results depend on hardware configuration and soft switch settings",
        "`FLASH`"
    ],
    "tok_fn": [
        "call a user function, also used in function definition",
        "`FN name(aexpr)`",
        {
            "examples": [
                "DEF FN CUBE(X) = X^3",
                "Y = FN CUBE(3)"
            ]
        }
    ],
    "tok_for": [
        "start a loop indexing on the given variable",
        "`FOR name = aexpr TO aexpr [STEP aexpr]`"
    ],
    "tok_fre": [
        "Return remaining memory in bytes.  Argument is ignored but must be a valid expression.  This also forces garbage collection of strings.",
        "`FRE (expr)`"
    ],
    "tok_get": [
        "Get a number or character from a keypress.  Displays blinking prompt, otherwise display is not affected.",
        "`GET var`"
    ],
    "tok_gosub": [
        "Execute the subroutine starting at the given line number.  Variables cannot be used.",
        "`GOSUB linenum`"
    ],
    "tok_goto": [
        "Branch to the given line number.  Variables cannot be used.",
        "`GOTO linenum`"
    ],
    "tok_gr": [
        "Switch to low resolution graphics and clear the screen.",
        "`GR`"
    ],
    "tok_hcoloreq": [
        "Set the color for high resolution graphics.",
        "`HCOLOR = aexpr`"
    ],
    "tok_hgr": [
        "Switch to high resolution graphics page 1 and clear the screen.",
        "`HGR`"
    ],
    "tok_hgr2": [
        "Switch to high resolution graphics page 2 and clear the screen.",
        "`HGR2`"
    ],
    "tok_himem": [
        "Set the highest address available to the Applesoft program.",
        "`HIMEM: aexpr`"
    ],
    "tok_hlin": [
        "Draw a horizontal line on the low resolution screen.",
        "`HLIN aexpr,aexpr AT aexpr`"
    ],
    "tok_home": [
        "Clear the text screen and move cursor to top left.",
        "`HOME`"
    ],
    "tok_hplot": [
        "Plot a point or line on the high resolution screen in the current color.",
        "`HPLOT aexpr,aexpr [{TO aexpr,aexpr}]`",
        "`HPLOT TO aexpr,aexpr [{TO aexpr,aexpr}]`"
    ],
    "tok_htab": [
        "move cursor to the given column, numbered from 1",
        "`HTAB aexpr`"
    ],
    "tok_if": [
        "Execute all statements following `THEN` (on the same line) if the condition is true. There are some abbreviated forms for branching.",
        "`IF expr THEN statement[{:statement}]`",
        {
            "examples": [
                "IF X<Y THEN X = 0: Y = 0",
                "IF A$ = \"Y\" THEN GOTO 100",
                "IF A$ = \"Y\" THEN 100",
                "IF A$ = \"Y\" GOTO 100"
            ]
        }
    ],
    "tok_inn": [
        "Switch input to a numbered expansion slot.",
        "`IN# aexpr`"
    ],
    "tok_input": [
        "Read from the current input device, optionally with prompt",
        "`INPUT [string;]var[{,var}]`",
        {
            "examples": [
                "INPUT PRICE",
                "INPUT MNTH%, DAY%, YEAR%",
                "INPUT \"WHAT IS YOUR PASSWORD? \"; PASSWD$"
            ]
        }
    ],
    "tok_int": [
        "integer part of argument.",
        "`INT (aexpr)`"
    ],
    "tok_inverse": [
        "Switch text to inverse video. Results depend on hardware and soft switch settings.",
        "`INVERSE`"
    ],
    "tok_left": [
        "substring starting from the beginning of a string",
        "`LEFT$ (sexpr,aexpr)`"
    ],
    "tok_len": [
        "length of a string",
        "`LEN (sexpr)`"
    ],
    "tok_let": [
        "`LET` is optional in assignments"
    ],
    "tok_list": [
        "output program listing to current device",
        "`LIST [linenum] [-linenum]`",
        "`LIST [linenum] [,linenum]`"
    ],
    "tok_load": [
        "load a program from tape or disk",
        "`LOAD [name]`"
    ],
    "tok_log": [
        "natural logarithm",
        "`LOG (aexpr)`"
    ],
    "tok_lomem": [
        "lower boundary in memory for variables",
        "`LOMEM: aexpr`"
    ],
    "tok_mid": [
        "return substring, `aexpr` arguments are start (indexed from 1) and length",
        "`MID$ (sexpr,aexpr[,aexpr])`"
    ],
    "tok_new": [
        "clear program and reset all variables and internal states",
        "`NEW`"
    ],
    "tok_next": [
        "Mark the end of a loop. Specifying loop variable is optional.",
        "`NEXT [avar[{,avar}]]`"
    ],
    "tok_normal": [
        "display text normally, cancels `INVERSE` and `FLASH`",
        "`NORMAL`"
    ],
    "tok_notrace": [
        "cancel display of line numbers during execution",
        "`NOTRACE`"
    ],
    "tok_on": [
        "Branch on a variable with non-zero integer values.  Also works with subroutines.",
        "`ON aexpr GOTO linenum[{,linenum}]`",
        "`ON aexpr GOSUB linenum[{,linenum}]`"
    ],
    "tok_onerr": [
        "Set error handling routine. There are some issues, see references.",
        "`ONERR GOTO linenum`"
    ],
    "tok_pdl": [
        "Read the dial on the given game paddle.",
        "`PDL (aexpr)`"
    ],
    "tok_peek": [
        "byte value at the given decimal address",
        "`PEEK (aexpr)`"
    ],
    "tok_plot": [
        "display low resolution pixel",
        "`PLOT aexpr,aexpr`"
    ],
    "tok_poke": [
        "set byte value at the given decimal address",
        "`POKE aexpr,aexpr`"
    ],
    "tok_pop": [
        "remove the most recent return address from the stack",
        "`POP`"
    ],
    "tok_pos": [
        "horizontal position of text cursor, argument is ignored, but must be a valid expression",
        "`POS(expr)`"
    ],
    "tok_prn": [
        "switch output to the given numbered expansion slot",
        "`PR# aexpr`"
    ],
    "tok_print": [
        "Write to the current output device.",
        "`PRINT [{expr[,|;]}]`",
        {
            "examples": [
                "PRINT",
                "PRINT A$, \"X = \";X"
            ]
        }
    ],
    "tok_read": [
        "read `DATA` values into variables",
        "`READ var[{,var}]`"
    ],
    "tok_recall": [
        "read values from tape into an array",
        "`RECALL name[%]`"
    ],
    "tok_rem": [
        "start of a comment (remark)",
        "`REM {character}`"
    ],
    "tok_restore": [
        "reset `DATA` to the beginning",
        "`RESTORE`"
    ],
    "tok_resume": [
        "Used at end of error handler to resume with the statement where the error occurred.  There are some issues, see the references.",
        "`RESUME`"
    ],
    "tok_return": [
        "return from subroutine",
        "`RETURN`"
    ],
    "tok_right": [
        "substring counting from the right",
        "`RIGHT$ (sexpr,aexpr)`"
    ],
    "tok_rnd": [
        "This is a uniform deviate between 0 and 1. Positive arguments change the seed",
        "`RND (aexpr)`"
    ],
    "tok_roteq": [
        "Set rotation for `DRAW` or `XDRAW`. See references for angular units.",
        "`ROT = aexpr`"
    ],
    "tok_run": [
        "`RUN` can be used in a program, but all variables are reset.",
        "`RUN [linenum|name]`"
    ],
    "tok_save": [
        "save program to disk or tape",
        "`SAVE [name]`"
    ],
    "tok_scaleeq": [
        "set scale for `DRAW` or `XDRAW`",
        "`SCALE = aexpr`"
    ],
    "tok_scrnp": [
        "color code at position on low resolution graphics screen",
        "`SCRN (aexpr,aexpr)`"
    ],
    "tok_sgn": [
        "sign function, gives -1,0, or 1",
        "`SGN (aexpr)`"
    ],
    "tok_shload": [
        "load shape from tape",
        "`SHLOAD`"
    ],
    "tok_sin": [
        "sine, the argument is in radians",
        "`SIN (aexpr)`"
    ],
    "tok_spcp": [
        "print number of spaces given in argument",
        "`SPC (aexpr)`"
    ],
    "tok_speedeq": [
        "set rate of printing to output device",
        "`SPEED = aexpr`"
    ],
    "tok_sqr": [
        "positive square root",
        "`SQR (aexpr)`"
    ],
    "tok_stop": [
        "terminate execution with a message giving the line number",
        "`STOP`"
    ],
    "tok_store": [
        "save array values to tape",
        "`STORE name[%]`"
    ],
    "tok_str": [
        "convert number to string",
        "`STR$ (aexpr)`"
    ],
    "tok_tabp": [
        "move text cursor to given column, numbered from 1",
        "`TAB (aexpr)`"
    ],
    "tok_tan": [
        "tangent, argument is in radians",
        "`TAN (aexpr)`"
    ],
    "tok_text": [
        "switch display to text",
        "`TEXT`"
    ],
    "tok_then": [
        "separates conditional from action within `IF` statement",
        "`THEN`"
    ],
    "tok_trace": [
        "display each line number during execution",
        "`TRACE`"
    ],
    "tok_usr": [
        "call machine language routine supplied by the user, passing the given argument",
        "`USR (aexpr)`"
    ],
    "tok_val": [
        "convert string to number",
        "`VAL (sexpr)`"
    ],
    "tok_vlin": [
        "draw a vertical line on the low resolution screen",
        "`VLIN aexpr,aexpr AT aexpr`"
    ],
    "tok_vtab": [
        "move text cursor to the given row, numbered from 1",
        "`VTAB aexpr`"
    ],
    "tok_wait": [
        "Suspend execution until bit pattern appears at given address. First argument is address, second is mask giving bits to test, third is a mask giving the expected bit values.",
        "`WAIT aexpr,aexpr[,aexpr]`"
    ],
    "tok_xdraw": [
        "Draw a shape using colors complementary to those currently on the screen. This can be used to erase a previously drawn shape.",
        "`XDRAW aexpr [AT aexpr,aexpr]`"
    ]
}"#;

pub struct StatementHovers {
    smap: HashMap<String,String> 
}

impl StatementHovers {
    pub fn new() -> Self {
        let mut ans = HashMap::new();
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(STATEMENTS_JSON) {
            if let Some(map) = obj.as_object() {
                for (kind,val) in map {
                    let mut hov = String::new();
                    if let Some(list) = val.as_array() {
                        for val in list {
                            if let Some(s) = val.as_str() {
                                hov += s;
                                hov += "\n\n";
                            }
                            if let Some(obj) = val.as_object() {
                                if let Some(ex_list) = obj["examples"].as_array() {
                                    hov += "#### examples\n";
                                    for ex in ex_list {
                                        if let Some(s) = ex.as_str() {
                                            hov += "\n    ";
                                            hov += s;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ans.insert(kind.to_string(),hov);
                }
            }
        }
        Self {
            smap: ans
        }
    }
    pub fn get(&self,node_kind: &str) -> Option<String> {
        self.smap.get(node_kind).cloned()
    }
}
