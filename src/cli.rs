use clap::{arg, crate_version, Arg, ArgAction, ArgGroup, Command, ValueHint};

const RNG_HELP: &str = "some types support ranges using `..` and `,,` separators,
e.g., `1..4,,7..10` would mean 1,2,3,7,8,9";
const IN_HELP: &str = "if disk image is piped, omit `--dimg` option";

pub fn build_cli() -> Command {
    let long_help = "a2kit is always invoked with exactly one of several subcommands.
The subcommands are generally designed to function as nodes in a pipeline.
PowerShell users may need to wrap the pipeline in a native shell.
Set RUST_LOG environment variable to control logging level.
  levels: trace,debug,info,warn,error

Examples:
---------
create DOS image:      `a2kit mkdsk -o dos33 -v 254 -t woz2 -d myimg.woz`
create ProDOS image:   `a2kit mkdsk -o prodos -v disk.new -t woz2 -d myimg.woz`
Language line entry:   `a2kit verify -t atxt`
Language file check:   `a2kit get -f myprog.bas | a2kit verify -t atxt`
Tokenize to file:      `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt > prog.atok
Tokenize to image:     `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt \\
                           | a2kit put -f prog -t atok -d myimg.dsk`
Detokenize from image: `a2kit get -f prog -t atok -d myimg.dsk | a2kit detokenize -t atok";
    let img_types = [
        "d13", "do", "po", "woz1", "woz2", "imd", "img", "2mg", "nib", "td0",
    ];
    let wrap_types = ["do", "po", "nib"];
    let os_names = ["cpm2", "cpm3", "dos32", "dos33", "prodos", "pascal", "fat"];
    let disk_kinds = [
        "8in",
        "8in-trs80",
        "8in-nabu",
        "5.25in",
        "5.25in-ibm-ssdd8",
        "5.25in-ibm-ssdd9",
        "5.25in-ibm-dsdd8",
        "5.25in-ibm-dsdd9",
        "5.25in-ibm-ssqd",
        "5.25in-ibm-dsqd",
        "5.25in-ibm-dshd",
        "5.25in-kayii",
        "5.25in-kay4",
        "5.25in-osb-sd",
        "5.25in-osb-dd",
        "3.5in",
        "3.5in-ss",
        "3.5in-ds",
        "3.5in-ibm-720",
        "3.5in-ibm-1440",
        "3.5in-ibm-2880",
        "3in-amstrad",
        "hdmax",
    ];
    let get_put_types = [
        "any",
        "auto",
        "bin",
        "txt",
        "raw",
        "rec",
        "atok",
        "itok",
        "mtok",
        "block",
        "sec",
        "track",
        "raw_track",
        "meta",
    ];

    let mut main_cmd = Command::new("a2kit")
        .about("Manipulates retro files and disk images with emphasis on Apple II.")
        .after_long_help(long_help)
        .version(crate_version!());
    main_cmd = main_cmd.subcommand(
        Command::new("mkdsk")
            .arg(arg!(-v --volume <VOLUME> "volume name or number").required(false))
            .arg(
                arg!(-t --type <TYPE> "type of disk image to create")
                    .required(true)
                    .value_parser(img_types),
            )
            .arg(
                arg!(-o --os <OS> "operating system format")
                    .required(true)
                    .value_parser(os_names),
            )
            .arg(arg!(-b --bootable "make disk bootable").action(ArgAction::SetTrue))
            .arg(
                arg!(-k --kind <SIZE> "kind of disk")
                    .value_parser(disk_kinds)
                    .required(false)
                    .default_value("5.25in"),
            )
            .arg(
                arg!(-d --dimg <PATH> "disk image path to create")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .arg(
                arg!(-w --wrap <TYPE> "type of disk image to wrap")
                    .value_parser(wrap_types)
                    .required(false),
            )
            .about("write a blank disk image to the given path"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("mkdir")
            .arg(arg!(-f --file <PATH> "path inside disk image of new directory").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("create a new directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("delete")
            .arg(arg!(-f --file <PATH> "path inside disk image to delete").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .visible_alias("del")
            .visible_alias("era")
            .about("delete a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("protect")
            .arg(arg!(-f --file <PATH> "path inside disk image to protect").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .arg(arg!(-p --password <PASSWORD> "password to assign").required(true))
            .arg(arg!(--read "protect read").action(ArgAction::SetTrue))
            .arg(arg!(--write "protect read").action(ArgAction::SetTrue))
            .arg(arg!(--delete "protect read").action(ArgAction::SetTrue))
            .about("password protect a disk or file"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("unprotect")
            .arg(arg!(-f --file <PATH> "path inside disk image to unprotect").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("remove password protection from a disk or file"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("lock")
            .arg(arg!(-f --file <PATH> "path inside disk image to lock").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("write protect a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("unlock")
            .arg(arg!(-f --file <PATH> "path inside disk image to unlock").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("remove write protection from a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("rename")
            .arg(arg!(-f --file <PATH> "path inside disk image to rename").required(true))
            .arg(arg!(-n --name <NAME> "new name").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("rename a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("retype")
            .arg(arg!(-f --file <PATH> "path inside disk image to retype").required(true))
            .arg(arg!(-t --type <TYPE> "file system type, code or mnemonic").required(true))
            .arg(arg!(-a --aux <AUX> "file system auxiliary metadata").required(true))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image itself")
                    .value_hint(ValueHint::FilePath)
                    .required(true),
            )
            .about("change file type inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("verify")
            .arg(
                arg!(-t --type <TYPE> "type of the file")
                    .required(true)
                    .value_parser(["atxt", "itxt", "mtxt"]),
            )
            .arg(
                arg!(-s --sexpr "write S-expressions to stderr").action(ArgAction::SetTrue)
            )
            .arg(
                arg!(-c --config <JSON> "modify diagnostic configuration")
                    .required(false)
                    .default_value(""),
            )
            .arg(
                arg!(-w --workspace <PATH> "workspace directory")
                    .required(false)
            )
            .about("read from stdin and error check"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("minify")
            .arg(
                arg!(-t --type <TYPE> "type of the file")
                    .required(true)
                    .value_parser(["atxt"]),
            )
            .arg(
                arg!(--level <LEVEL> "set minification level")
                    .value_parser(["0", "1", "2", "3"])
                    .default_value("1"),
            )
            .arg(arg!(--flags <VAL> "set minification flags").default_value("1"))
            .group(
                ArgGroup::new("opt")
                    .required(false)
                    .multiple(false)
                    .args(["level", "flags"]),
            )
            .about("reduce program size"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("renumber")
            .arg(
                arg!(-t --type <TYPE> "type of the file")
                    .required(true)
                    .value_parser(["atxt","itxt"]),
            )
            .arg(arg!(-b --beg <NUM> "lowest number to renumber").required(true))
            .arg(arg!(-e --end <NUM> "highest number to renumber plus 1").required(true))
            .arg(arg!(-f --first <NUM> "first number").required(true))
            .arg(arg!(-s --step <NUM> "step between numbers").required(true))
            .arg(arg!(-r --reorder "allow reordering of lines").action(ArgAction::SetTrue))
            .about("renumber BASIC program lines"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("get")
            .arg(
                arg!(-f --file <PATH> "path, key, or address, maybe inside disk image")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .arg(
                arg!(-t --type <TYPE> "type of the item")
                    .required(false)
                    .value_parser(get_put_types),
            )
            .arg(
                arg!(-d --dimg <PATH> "path to disk image")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .arg(
                arg!(-l --len <LENGTH> "length of record in DOS 3.3 random access text file")
                    .required(false),
            )
            .arg(arg!(--trunc "truncate raw at EOF if possible").action(ArgAction::SetTrue))
            .about("read from stdin, local, or disk image, write to stdout")
            .after_help(RNG_HELP.to_string() + "\n\n" + IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("put")
            .arg(
                arg!(-f --file <PATH> "path, key, or address, maybe inside disk image")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .arg(
                arg!(-t --type <TYPE> "type of the item")
                    .required(false)
                    .value_parser(get_put_types),
            )
            .arg(
                arg!(-d --dimg <PATH> "path to disk image")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .arg(arg!(-a --addr <ADDRESS> "address of binary file").required(false))
            .about("read from stdin, write to local or disk image")
            .after_help(RNG_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("catalog")
            .arg(arg!(-f --file <PATH> "path of directory inside disk image").required(false))
            .arg(arg!(--generic "use generic output format").action(ArgAction::SetTrue))
            .arg(
                arg!(-d --dimg <PATH> "path to disk image")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .visible_alias("cat")
            .visible_alias("dir")
            .visible_alias("ls")
            .about("write disk image catalog to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("tree")
            .arg(
                Arg::new("dimg").short('d').long("dimg").help("path to disk image").value_name("PATH")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .arg(Arg::new("meta").long("meta").help("include metadata").action(ArgAction::SetTrue))
            .about("write directory tree as a JSON string to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("stat")
            .arg(
                Arg::new("dimg").short('d').long("dimg").help("path to disk image").value_name("PATH")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .about("write FS statistics as a JSON string to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("geometry")
            .arg(
                Arg::new("dimg").short('d').long("dimg").help("path to disk image").value_name("PATH")
                    .value_hint(ValueHint::FilePath)
                    .required(false),
            )
            .about("write disk geometry as a JSON string to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("tokenize")
            .arg(
                Arg::new("addr").short('a').long("addr").help("address of tokenized code (Applesoft only)").value_name("ADDRESS")
                    .required(false),
            )
            .arg(
                Arg::new("type").short('t').long("type").help("type of the file").value_name("TYPE")
                    .required(true)
                    .value_parser(["atxt", "itxt", "mtxt"]),
            )
            .visible_alias("tok")
            .about("read from stdin, tokenize, write to stdout"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("detokenize")
            .arg(
                Arg::new("type").short('t').long("type").help("type of the file").value_name("TYPE")
                    .required(true)
                    .value_parser(["atok", "itok", "mtok"]),
            )
            .visible_alias("dtok")
            .about("read from stdin, detokenize, write to stdout"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("asm")
            .arg(
                Arg::new("assembler").short('a').long("assembler").help("assembler variant").value_name("NAME")
                    .required(false)
                    .value_parser(["m8","m16","m16+","m32"])
                    .default_value("m8")
            )
            .arg(
                Arg::new("workspace").short('w').long("workspace").help("workspace directory").value_name("PATH")
                    .required(false)
            )
            .arg(
                Arg::new("literals").long("literals").help("assign values to disassembled hex labels").action(ArgAction::SetTrue)
            )
            .about("read from stdin, assemble, write to stdout")
            .after_help("At present this is limited, it will error out if program counter or symbol value cannot be determined.")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("dasm")
            .arg(
                Arg::new("proc").short('p').long("proc").help("processor target").value_name("NAME")
                    .required(true)
                    .value_parser(["6502","65c02","65802","65816"])
            )
            .arg(
                Arg::new("mx").long("mx").help("MX status bits").value_name("BINARY")
                    .required(false)
                    .value_parser(["00","01","10","11"])
                    .default_value("11")
            )
            .arg(
                Arg::new("org").short('o').long("org").help("starting address").value_name("ADDRESS")
                    .required(true)
            )
            .about("read from stdin, disassemble, write to stdout")
    );
    return main_cmd;
}
