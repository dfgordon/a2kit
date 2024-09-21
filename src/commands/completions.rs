use clap_complete::shells;
use crate::STDRESULT;

// const ALIASES: [(&str,&[&str]);4] = [
//     ("(catalog)", &["(ls)","(dir)","(cat)"]),
//     ("(delete)" , &["(del)","(era)"]),
//     ("(tokenize)" , &["(tok)"]),
//     ("(detokenize)" , &["(dtok)"])
// ];

// fn refine_zsh(script: &str) -> String {
//     let aliases = std::collections::HashMap::from(ALIASES);
//     let eq_patt = regex::RegexBuilder::new(r"^'--(\w+)=\[").multi_line(true).build().expect("regex parsing error");
//     let alias_patt = regex::Regex::new(r"^\(\w+\)$").expect("regex parsing error");
//     let intermediate = eq_patt.replace_all(script, "'--$1+[");
//     let mut new_script = String::new();
//     let mut accum = String::new();
//     let mut curr_cmd = String::new();
//     let mut alias_list : &[&str] = aliases.get("(catalog)").unwrap();
//     for line in intermediate.lines() {
//         match alias_patt.find(line) {
//             Some(res) if aliases.contains_key(res.as_str()) => {
//                 accum = line.to_string();
//                 accum += "\n";
//                 alias_list = aliases.get(res.as_str()).unwrap();
//                 curr_cmd = res.as_str().to_string();
//             },
//             _ => {
//                 if accum.len() > 0 {
//                     accum += line;
//                     accum += "\n";
//                     if line==";;" {
//                         new_script += &accum;
//                         for alias in alias_list {
//                             new_script += &accum.replace(&curr_cmd,alias);
//                         }
//                         accum = "".to_string();
//                     }
//                 } else {
//                     new_script += line;
//                     new_script += "\n";
//                 }
//             }
//         }
//     }
//     return new_script;
// }

pub fn generate(mut main_cmd: clap::Command,cmd: &clap::ArgMatches) -> STDRESULT {
    match cmd.get_one::<String>("shell").unwrap().as_str() {
        "bash" => clap_complete::generate(shells::Bash,&mut main_cmd,"a2kit",&mut std::io::stdout()),
        "elv" => clap_complete::generate(shells::Elvish,&mut main_cmd,"a2kit",&mut std::io::stdout()),
        "fish" => clap_complete::generate(shells::Fish,&mut main_cmd,"a2kit",&mut std::io::stdout()),
        "ps1" => clap_complete::generate(shells::PowerShell,&mut main_cmd,"a2kit",&mut std::io::stdout()),
        "zsh" => clap_complete::generate(shells::Zsh,&mut main_cmd,"a2kit",&mut std::io::stdout()),
        _ => panic!("unexpected shell")
    }
    Ok(())
}
