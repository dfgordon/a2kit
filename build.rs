use clap::ValueEnum;

include!("src/cli.rs");

const ALIASES: [(&str,&[&str]);4] = [
    ("(catalog)", &["(ls)","(dir)","(cat)"]),
    ("(delete)" , &["(del)","(era)"]),
    ("(tokenize)" , &["(tok)"]),
    ("(detokenize)" , &["(dtok)"])
];

fn refine_zsh(script: &str) -> String {
    let aliases = std::collections::HashMap::from(ALIASES);
    let eq_patt = regex::RegexBuilder::new(r"^'--(\w+)=\[").multi_line(true).build().expect("regex parsing error");
    let alias_patt = regex::Regex::new(r"^\(\w+\)$").expect("regex parsing error");
    let intermediate = eq_patt.replace_all(script, "'--$1+[");
    let mut new_script = String::new();
    let mut accum = String::new();
    let mut curr_cmd = String::new();
    let mut alias_list : &[&str] = aliases.get("(catalog)").unwrap();
    for line in intermediate.lines() {
        match alias_patt.find(line) {
            Some(res) if aliases.contains_key(res.as_str()) => {
                accum = line.to_string();
                accum += "\n";
                alias_list = aliases.get(res.as_str()).unwrap();
                curr_cmd = res.as_str().to_string();
            },
            _ => {
                if accum.len() > 0 {
                    accum += line;
                    accum += "\n";
                    if line==";;" {
                        new_script += &accum;
                        for alias in alias_list {
                            new_script += &accum.replace(&curr_cmd,alias);
                        }
                        accum = "".to_string();
                    }
                } else {
                    new_script += line;
                    new_script += "\n";
                }
            }
        }
    }
    return new_script;
}

fn main() -> Result<(), std::io::Error> {
    if std::env::var("DOCS_RS").is_err() {
        let outdir = match std::env::var_os("CARGO_MANIFEST_DIR") {
            None => return Ok(()),
            Some(root) => std::path::Path::new(&root).join("completions"),
        };

        let mut cmd = build_cli();

        for &shell in clap_complete::Shell::value_variants() {
            clap_complete::generate_to(shell, &mut cmd, "a2kit", &outdir)?;
            match shell {
                clap_complete::Shell::Zsh => {
                    let s = std::fs::read(outdir.join("_a2kit")).expect("zsh completions missing");
                    let script = String::from_utf8(s).expect("clap_complete output not UTF8");
                    let refined = refine_zsh(&script);               
                    std::fs::write(outdir.join("_a2kit"),refined)?;
                },
                _ => {}
            }
        }
    }

    Ok(())
}