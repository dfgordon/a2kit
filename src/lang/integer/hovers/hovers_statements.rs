use serde_json;
use std::collections::HashMap;

// The ordering of the inner list matters.
const STATEMENTS_JSON: &str = r#"
{
    "com_load": [
        "Load file from tape",
        "`LOAD`"
    ],
    "com_save": [
        "Save file to tape",
        "`SAVE`"
    ],
    "com_con": [
        "Continue program after stopping with Ctrl-C",
        "`CON`"
    ],
    "com_run": [
        "Run program starting at begining or at line number",
        "`RUN [aexpr]`"
    ],
    "com_del": [
        "Delete program line or line range",
        "`DEL linenum[,linenum]`"
    ],
    "com_new": [
        "Clear program and variables",
        "`NEW`"
    ],
    "com_clr": [
        "Clear variables",
        "`CLR`"
    ],
    "com_auto": [
        "Set automatic line numbering",
        "`AUTO num[,num]`"
    ],
    "com_man": [
        "Stop automatic line numbering (after Ctrl-X)",
        "`MAN`"
    ],
    "com_himem": [
        "Set highest address used by BASIC, program builds down from the top of memory",
        "`HIMEM: aexpr`"
    ],
    "com_lomem": [
        "Set lowest address used by BASIC, variables build up from bottom of memory (destroys variables)",
        "`LOMEM: aexpr`"
    ],
    "fcall_abs": [
        "absolute value",
        "`ABS (aexpr)`"
    ],
    "fcall_ascp": [
        "ASCII code of first character",
        "`ASC (sexpr)`"
    ],
    "statement_call": [
        "Call machine language subroutine at decimal address.",
        "`CALL aexpr`",
        {
            "examples": [
                "CALL 768"
            ]
        }
    ],
    "statement_coloreq": [
        "Set the low resolution color",
        "`COLOR = aexpr`"
    ],
    "statement_dim": [
        "allocate space for arrays or strings",
        "`DIM name[$] (aexpr) [{,name[$] (aexpr)}]`"
    ],
    "statement_dsp": [
        "watch variable during execution",
        "`DSP name[$]`"
    ],
    "statement_end": [
        "stop program execution",
        "`END`"
    ],
    "statement_for": [
        "start a loop indexing on the given variable",
        "`FOR name = aexpr TO aexpr [STEP aexpr]`"
    ],
    "statement_gosub": [
        "Execute the subroutine starting at the given line number, which can be an expression.",
        "`GOSUB aexpr`"
    ],
    "statement_goto": [
        "Branch to the given line number, which can be an expression.",
        "`GOTO aexpr`"
    ],
    "statement_gr": [
        "Switch to low resolution graphics and clear the screen.",
        "`GR`"
    ],
    "statement_hlin": [
        "Draw a horizontal line on the low resolution screen.",
        "`HLIN aexpr,aexpr AT aexpr`"
    ],
    "statement_if": [
        "Execute statement following `THEN` if the condition is true. If `THEN` does not branch, a trailing statment always executes.",
        "`IF expr THEN statement[{:statement}]`",
        {
            "examples": [
                "IF X<Y THEN X = 0: Y = 0",
                "IF A$ = \"Y\" THEN GOTO 100",
                "IF A$ = \"Y\" THEN 100"
            ]
        }
    ],
    "statement_inn": [
        "Switch input to a numbered expansion slot.",
        "`IN# aexpr`"
    ],
    "statement_input": [
        "Read from the current input device, optionally with prompt",
        "`INPUT [sexpr,]var[{,var}]`",
        {
            "examples": [
                "INPUT PRICE",
                "INPUT \"WHAT IS YOUR PASSWORD? \", PASSWD$"
            ]
        }
    ],
    "fcall_lenp": [
        "length of a string",
        "`LEN (sexpr)`"
    ],
    "statement_let": [
        "`LET` is optional in assignments. It can widen the range of acceptable variable names."
    ],
    "statement_list": [
        "output program listing to current device",
        "`LIST [linenum] [,linenum]`"
    ],
    "statement_next": [
        "Mark the end of a loop.",
        "`NEXT avar[{,avar}]`"
    ],
    "statement_nodsp": [
        "Do not watch variable during execution.",
        "`NODSP name[$]`"
    ],
    "statement_notrace": [
        "cancel display of line numbers during execution",
        "`NOTRACE`"
    ],
    "fcall_pdl": [
        "Read the dial on the given game paddle.",
        "`PDL (aexpr)`"
    ],
    "fcall_peek": [
        "byte value at the given decimal address",
        "`PEEK (aexpr)`"
    ],
    "statement_plot": [
        "display low resolution pixel",
        "`PLOT aexpr,aexpr`"
    ],
    "statement_poke": [
        "set byte value at the given decimal address",
        "`POKE aexpr,aexpr`"
    ],
    "statement_pop": [
        "remove the most recent return address from the stack",
        "`POP`"
    ],
    "statement_prn": [
        "switch output to the given numbered expansion slot",
        "`PR# aexpr`"
    ],
    "statement_print": [
        "Write to the current output device.",
        "`PRINT [expr[{,expr|;expr|,|;}]]`",
        {
            "examples": [
                "PRINT",
                "PRINT A$, \"X = \";X"
            ]
        }
    ],
    "statement_rem": [
        "start of a comment (remark)",
        "`REM {character}`"
    ],
    "statement_return": [
        "return from subroutine",
        "`RETURN`"
    ],
    "fcall_rnd": [
        "Uniform deviate between 0 and aexpr-1 if aexpr>0, between 0 and aexpr+1 otherwise.",
        "`RND (aexpr)`"
    ],
    "fcall_scrnp": [
        "color code at position on low resolution graphics screen",
        "`SCRN (aexpr,aexpr)`"
    ],
    "fcall_sgn": [
        "sign function, gives -1,0, or 1",
        "`SGN (aexpr)`"
    ],
    "statement_tab": [
        "move text cursor to given column, numbered from 1",
        "`TAB aexpr`"
    ],
    "statement_text": [
        "switch display to text",
        "`TEXT`"
    ],
    "statement_then": [
        "see `IF`"
    ],
    "statement_trace": [
        "display each line number during execution",
        "`TRACE`"
    ],
    "statement_vlin": [
        "draw a vertical line on the low resolution screen",
        "`VLIN aexpr,aexpr AT aexpr`"
    ],
    "statement_vtab": [
        "move text cursor to the given row, numbered from 1",
        "`VTAB aexpr`"
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
        let parts = node_kind.split('_').collect::<Vec<&str>>();
        if parts.len()>1 {
            return self.smap.get(&[parts[0],"_",parts[1]].concat()).cloned()
        }
        None
    }
}
