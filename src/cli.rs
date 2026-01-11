use clap::{value_parser, crate_version, Arg, ArgAction, ArgGroup, Command, ValueHint};

const RNG_HELP: &str = "some types support ranges using `..` and `,,` separators,
e.g., `1..4,,7..10` would mean 1,2,3,7,8,9";
const IN_HELP: &str = "if disk image is piped, omit `--dimg` option";
const WOZ_HELP: &str = "for WOZ you can use quarter-decimals for cylinder numbers";
const F_LONG_HELP: &str = "interpretation depends on type, for files this is
the usual notion of a path, for disk regions it is a numerical address (e.g. <cyl>,<head>,<sec>),
for metadata it is a key path (e.g. /woz2/info)";
const T_LONG_HELP: &str = "Types are broadly separated into file, disk region, and metadata categories.
The `any` type is a generalized representation of a file that works with all supported file systems.
The `auto` type will try to heuristically select a type using file system hints and content.";
const PRO_LONG_HELP: &str = "Use the proprietary track format that is described in the file at PATH.
The file should contain a JSON string describing a GCR soft sectoring scheme.
To get an example of this file use `a2kit geometry --abstract ...` on a standard WOZ";

fn file_arg(help: &'static str, req: bool, shell_hint: bool) -> Arg {
    let ans = Arg::new("file").short('f').long("file").value_name("PATH").required(req).help(help);
    if shell_hint {
        ans.value_hint(ValueHint::FilePath)
    } else {
        ans
    }
}

fn pro_arg() -> Arg {
    Arg::new("pro").long("pro").value_name("PATH").help("use proprietary track format")
                .long_help(PRO_LONG_HELP)
                .value_hint(ValueHint::FilePath)
                .required(false)
}

fn extern_arg() -> Arg {
    Arg::new("extern").long("extern").value_name("LIST").help("external references")
        .required(false)
        .value_delimiter(',')
        .value_parser(0..0xffff)
        .long_help("comma delimited list of line numbers that are referenced externally")
}

fn console_arg() -> Arg {
    Arg::new("console").long("console").help("format for console unconditionally")
        .required(false)
        .action(ArgAction::SetTrue)
        .long_help("even if the output context is a file or pipe, format it for the console")
}

fn indent_arg() -> Arg {
    Arg::new("indent").long("indent").help("JSON indentation, omit to minify")
        .value_name("SPACES")
        .value_parser(value_parser!(u16).range(0..16))
        .required(false)
}

fn dimg_arg(req: bool) -> Arg {
    Arg::new("dimg").short('d').long("dimg").help("path to disk image itself")
        .value_name("PATH")
        .value_hint(ValueHint::FilePath)
        .required(req)
}

fn method_arg() -> Arg {
    Arg::new("method").long("method").help("select decoding methodology")
        .value_name("METHOD")
        .value_parser(["auto","fast","analyze","emulate"])
        .required(false)
        .default_value("auto")
}

pub fn build_cli() -> Command {
    let long_help = "a2kit is always invoked with exactly one of several subcommands.
The subcommands are generally designed to function as nodes in a pipeline.
PowerShell users should use version 7.4 or higher to avoid lots of trouble.
Set RUST_LOG environment variable to control logging level.
  levels: trace,debug,info,warn,error

Examples:
---------
smart copy:            `a2kit cp myimg.woz/program ./program.bas`
create DOS image:      `a2kit mkdsk -o dos33 -v 254 -t woz2 -d myimg.woz`
create ProDOS image:   `a2kit mkdsk -o prodos -v disk.new -t woz2 -d myimg.woz`
language line entry:   `a2kit verify -t atxt`
language file check:   `a2kit get -f myprog.bas | a2kit verify -t atxt`
detokenize from image: `a2kit get -f prog -t atok -d myimg.dsk | a2kit detokenize -t atok
tokenize to file:      `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt > prog.atok
tokenize to image:     `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt \\
                           | a2kit put -f prog -t atok -d myimg.dsk`
    ...or smart copy:  `a2kit cp -a 2049 prog.bas myimg.dsk";

    let img_types = [
        "d13", "do", "po", "woz1", "woz2", "imd", "img", "2mg", "nib", "td0",
    ];
    let wrap_types = ["do", "po", "nib"];
    let os_names = ["cpm2", "cpm3", "dos32", "dos33", "prodos", "pascal", "fat"];
    let disk_kinds = [
        "8in-ibm-sssd",
        "8in-trs80-ssdd",
        "8in-nabu-dsdd",
        "5.25in-apple-13",
        "5.25in-apple-16",
        "5.25in-ibm-ssdd8",
        "5.25in-ibm-ssdd9",
        "5.25in-ibm-dsdd8",
        "5.25in-ibm-dsdd9",
        "5.25in-ibm-ssqd",
        "5.25in-ibm-dsqd",
        "5.25in-ibm-dshd",
        "5.25in-kay-ssdd",
        "5.25in-kay-dsdd",
        "5.25in-osb-sssd",
        "5.25in-osb-ssdd",
        "3.5in-apple-400",
        "3.5in-apple-800",
        "3.5in-ibm-720",
        "3.5in-ibm-1440",
        "3.5in-ibm-2880",
        "3in-amstrad-ssdd",
        "hdmax",
    ];
    let get_put_types = [
        "any",
        "auto",
        "as",
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

    let pack_unpack_types = [
        "auto",
        "as",
        "bin",
        "txt",
        "raw",
        "rec",
        "atok",
        "itok",
        "mtok"
    ];

    let mut main_cmd = Command::new("a2kit")
        .about("Retro languages and disk images with emphasis on Apple II.")
        .after_long_help(long_help)
        .version(crate_version!());

    main_cmd = main_cmd.subcommand(
        Command::new("cp")
            .arg(Arg::new("paths").num_args(2..=1000).help("sequence of paths, last path is the destination").value_name("PATHS").required(true)
                .long_help("Paths inside the disk image always use the forward slash.
It is OK to have a mixture such as `c:\\path\\to\\disk.img/path/to/file`.
Recursive glob patterns like `disk.img/**` will expand correctly,
but the files will all go to the same target directory."))
            .arg(Arg::new("addr").long("addr").short('a').help("load-address if applicable").value_name("ADDRESS").required(false)
                .long_help("Specify the load address that is stored with some Apple file types.
This is only needed for host-to-image copies involving Apple file systems"))
            .arg(pro_arg())
            .arg(method_arg())
            .about("smart copy that formats for the target")
            .after_help("Disk images can appear midway through a path,
the path then continues inside the image (e.g. /path/to/mydisk.woz/startup).
Glob patterns outside a disk image are only expanded if the shell expands them,
whereas glob patterns inside a disk image are expanded by a2kit.
Delimiters like quotes are sometimes needed for the latter to work.
This subcommand is meant as a convenience in non-critical situations.
If you need to be precise the get/put syntax may be more appropriate.")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("get")
            .arg(file_arg("path, key, or address, maybe inside disk image",false,true).long_help(F_LONG_HELP))
            .arg(Arg::new("type").long("type").short('t').help("type of the item")
                .value_name("TYPE").required(false).value_parser(get_put_types).long_help(T_LONG_HELP)
            )
            .arg(dimg_arg(false))
            .arg(indent_arg())
            .arg(Arg::new("len").long("len").short('l').help("length of record in DOS 3.3 random access text file")
                .value_name("LENGTH").required(false)
            )
            .arg(Arg::new("trunc").long("trunc").help("truncate raw at EOF if possible").action(ArgAction::SetTrue))
            .arg(pro_arg())
            .arg(method_arg())
            .arg(Arg::new("explicit").long("explicit").help("supply an explicit hex address").value_name("HEX").required(false))
            .arg(console_arg())
            .about("read from stdin, local, or disk image, write to stdout")
            .after_help([RNG_HELP,"\n\n",WOZ_HELP,"\n\n",IN_HELP].concat())
    );
    main_cmd = main_cmd.subcommand(
        Command::new("put")
            .arg(file_arg("path, key, or address, maybe inside disk image",false,true).long_help(F_LONG_HELP))
            .arg(Arg::new("type").long("type").short('t').help("type of the item")
                .value_name("TYPE").required(false).value_parser(get_put_types).long_help(T_LONG_HELP)
            )
            .arg(dimg_arg(false))
            .arg(Arg::new("addr").long("addr").short('a').help("load-address if applicable").value_name("ADDRESS").required(false))
            .arg(pro_arg())
            .arg(method_arg())
            .arg(Arg::new("explicit").long("explicit").help("supply an explicit hex address").value_name("HEX").required(false))
            .about("read from stdin, write to local or disk image")
            .after_help([RNG_HELP,"\n\n",WOZ_HELP].concat())
    );
    main_cmd = main_cmd.subcommand(
        Command::new("mget")
            .arg(dimg_arg(true))
            .arg(indent_arg())
            .arg(pro_arg())
            .arg(method_arg())
            .about("read list of paths from stdin, get files from disk image, write file images to stdout")
            .after_help("this can take `a2kit glob` as a piped input")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("mput")
            .arg(dimg_arg(true))
            .arg(file_arg("override target paths",false,false))
            .arg(pro_arg())
            .arg(method_arg())
            .about("read list of file images from stdin, restore files to a disk image")
            .after_help("for CP/M the user number can be overridden using `-f <num>:`")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("pack")
            .arg(file_arg("target path for this file image",true,false))
            .arg(Arg::new("type").long("type").short('t').help("type of the item")
                .value_name("TYPE").required(true).value_parser(pack_unpack_types)
            )
            .arg(Arg::new("addr").long("addr").short('a').help("load-address if applicable").value_name("ADDRESS").required(false))
            .arg(Arg::new("block").long("block").short('b').help("size of block in bytes if needed")
                .value_name("BYTES")
                .value_parser(value_parser!(u16).range(128..=16384))
                .required(false)
            )
            .arg(Arg::new("os").long("os").short('o').help("operating system format").value_name("OS")
                    .required(true)
                    .value_parser(os_names)
            )
            .arg(indent_arg())
            .about("pack data into a file image")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("unpack")
            .arg(Arg::new("type").long("type").short('t').help("type of the item")
                .value_name("TYPE").required(true).value_parser(pack_unpack_types)
            )
            .arg(Arg::new("trunc").long("trunc").help("truncate raw at EOF if possible").action(ArgAction::SetTrue))
            .arg(Arg::new("len").long("len").short('l').help("length of record in DOS 3.3 random access text file")
                .value_name("LENGTH").required(false)
            )
            .arg(console_arg())
            .arg(indent_arg())
            .about("unpack data from a file image")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("mkdsk")
            .arg(Arg::new("volume").long("volume").short('v').value_name("VOLUME").help("volume name or number")
                .required(false))
            .arg(Arg::new("type").long("type").short('t').value_name("TYPE").help("type of disk image to create")
                .required(true)
                .value_parser(img_types),
            )
            .arg(Arg::new("os").long("os").short('o').value_name("OS").help("operating system format")
                .required(false)
                .value_parser(os_names),
            )
            .arg(Arg::new("empty").long("empty").help("wipe all sectors").action(ArgAction::SetTrue))
            .arg(Arg::new("blank").long("blank").help("medium is pristine").action(ArgAction::SetTrue))
            .arg(Arg::new("bootable").long("bootable").short('b').help("make disk bootable").action(ArgAction::SetTrue))
            .arg(Arg::new("kind").long("kind").short('k').value_name("PKG-VEND-FMT").help("kind of disk")
                .value_parser(disk_kinds)
                .required(false)
            )
            .arg(Arg::new("dimg").long("dimg").short('d').value_name("PATH").help("disk image path to create")
                .value_hint(ValueHint::FilePath)
                .required(true),
            )
            .arg(Arg::new("wrap").long("wrap").short('w').value_name("TYPE").help("type of disk image to wrap")
                .value_parser(wrap_types)
                .required(false),
            )
            .arg(pro_arg())
            .group(
                ArgGroup::new("contents")
                    .required(true)
                    .multiple(false)
                    .args(["os", "empty", "blank"]),
            )
            .arg(Arg::new("flux").long("flux").value_name("TRACKS").help("list of flux tracks")
                .required(false)
                .long_help("list of tracks (CH form) that will be created as flux streams, the `..` and `,,` separators can be used to form ranges")
            )
            .visible_alias("mkimg")
            .about("write a new disk image to the given path")
    );
    main_cmd = main_cmd.subcommand(
        Command::new("mkdir")
            .arg(file_arg("path inside disk image of new directory",true,false))
            .arg(dimg_arg(true))
            .arg(pro_arg())
            .arg(method_arg())
            .about("create a new directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("delete")
            .arg(file_arg("path inside disk image to delete",true,false))
            .arg(dimg_arg(true))
            .arg(pro_arg())
            .arg(method_arg())
            .visible_alias("del")
            .visible_alias("era")
            .about("delete a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("access")
            .arg(file_arg("path inside disk image where permissions will apply",true,false))
            .arg(dimg_arg(true))
            .arg(Arg::new("password").long("password").short('p').value_name("PASSWORD").help("password to assign").required(false))
            .arg(Arg::new("read").long("read").help("allow read").action(ArgAction::SetTrue))
            .arg(Arg::new("write").long("write").help("allow write").action(ArgAction::SetTrue))
            .arg(Arg::new("delete").long("delete").help("allow delete").action(ArgAction::SetTrue))
            .arg(Arg::new("rename").long("rename").help("allow rename").action(ArgAction::SetTrue))
            .arg(Arg::new("no-read").long("no-read").help("protect read").action(ArgAction::SetTrue))
            .arg(Arg::new("no-write").long("no-write").help("protect write").action(ArgAction::SetTrue))
            .arg(Arg::new("no-delete").long("no-delete").help("protect delete").action(ArgAction::SetTrue))
            .arg(Arg::new("no-rename").long("no-rename").help("protect rename").action(ArgAction::SetTrue))
            .group(ArgGroup::new("reading").args(["read","no-read"]).required(false))
            .group(ArgGroup::new("writing").args(["write","no-write"]).required(false))
            .group(ArgGroup::new("deleting").args(["delete","no-delete"]).required(false))
            .group(ArgGroup::new("renaming").args(["rename","no-rename"]).required(false))
            .arg(pro_arg())
            .arg(method_arg())
            .about("change file system permissions"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("rename")
            .arg(file_arg("path inside disk image to rename",true,false))
            .arg(Arg::new("name").long("name").short('n').value_name("NAME").help("new name").required(true))
            .arg(dimg_arg(true))
            .arg(pro_arg())
            .arg(method_arg())
            .about("rename a file or directory inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("retype")
            .arg(file_arg("path inside disk image to retype",true,false))
            .arg(Arg::new("type").long("type").short('t').value_name("TYPE").help("file system type, code or mnemonic").required(true))
            .arg(Arg::new("aux").long("aux").short('a').value_name("AUX").help("file system auxiliary metadata").required(true))
            .arg(dimg_arg(true))
            .arg(pro_arg())
            .arg(method_arg())
            .about("change file type inside a disk image"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("verify")
            .arg(Arg::new("type").long("type").short('t').value_name("TYPE").help("type of the file")
                    .required(true)
                    .value_parser(["atxt", "itxt", "mtxt"]),
            )
            .arg(Arg::new("sexpr").long("sexpr").short('s').help("write S-expressions to stderr").action(ArgAction::SetTrue))
            .arg(Arg::new("config").long("config").short('c').value_name("JSON").help("modify diagnostic configuration")
                .required(false)
                .default_value(""),
            )
            .arg(Arg::new("workspace").long("workspace").short('w').value_name("PATH").help("workspace directory")
                .value_hint(ValueHint::FilePath)
                .required(false)
            )
            .about("read from stdin and perform language analysis"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("minify")
            .arg(Arg::new("type").long("type").short('t').value_name("TYPE").help("type of the file")
                .required(true)
                .value_parser(["atxt"])
            )
            .arg(Arg::new("level").long("level").value_name("LEVEL").help("set minification level")
                .value_parser(["0", "1", "2", "3"])
                .default_value("1")
            )
            .arg(Arg::new("flags").long("flags").value_name("VAL").help("set minification flags").default_value("1"))
            .arg(extern_arg())
            .group(
                ArgGroup::new("opt")
                    .required(false)
                    .multiple(false)
                    .args(["level", "flags"])
            )
            .about("reduce program size")
            .after_help("level 0=identity, 1=intra-line, 2=delete, 3=combine"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("renumber")
            .arg(Arg::new("type").long("type").short('t').value_name("TYPE").help("type of the file")
                    .required(true)
                    .value_parser(["atxt","itxt"]),
            )
            .arg(Arg::new("beg").long("beg").short('b').value_name("NUM").help("lowest number to renumber").required(true))
            .arg(Arg::new("end").long("end").short('e').value_name("NUM").help("highest number to renumber plus 1").required(true))
            .arg(Arg::new("first").long("first").short('f').value_name("NUM").help("first number").required(true))
            .arg(Arg::new("step").long("step").short('s').value_name("NUM").help("step between numbers").required(true))
            .arg(Arg::new("reorder").long("reorder").short('r').help("allow reordering of lines").action(ArgAction::SetTrue))
            .arg(extern_arg())
            .about("renumber BASIC program lines"),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("catalog")
            .arg(file_arg("path of directory inside disk image",false,false))
            .arg(Arg::new("generic").long("generic").help("use generic output format").action(ArgAction::SetTrue))
            .arg(dimg_arg(false))
            .arg(pro_arg())
            .arg(method_arg())
            .visible_alias("cat")
            .visible_alias("dir")
            .visible_alias("ls")
            .about("write disk image catalog to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("tree")
            .arg(dimg_arg(false))
            .arg(Arg::new("meta").long("meta").help("include metadata").action(ArgAction::SetTrue))
            .arg(indent_arg())
            .arg(pro_arg())
            .arg(method_arg())
            .about("write directory tree as a JSON string to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("stat")
            .arg(dimg_arg(false))
            .arg(indent_arg())
            .arg(pro_arg())
            .arg(method_arg())
            .about("write FS statistics as a JSON string to stdout")
            .after_help(IN_HELP),
    );
    main_cmd = main_cmd.subcommand(
        Command::new("geometry")
            .arg(dimg_arg(false))
            .arg(indent_arg())
            .arg(pro_arg())
            .arg(method_arg())
            .arg(Arg::new("abstract").long("abstract").help("use abstract representation if available").action(ArgAction::SetTrue))
            .about("write disk format information as a JSON string to stdout")
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
            .arg(console_arg())
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
            .arg(console_arg())
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
    main_cmd = main_cmd.subcommand(
        Command::new("glob")
            .arg(dimg_arg(false))
            .arg(
                Arg::new("file").short('f').long("file").help("glob pattern to match against").value_name("PATTERN")
                    .required(true),
            )
            .arg(indent_arg())
            .arg(pro_arg())
            .arg(method_arg())
            .about("write JSON list of matching paths to stdout")
            .after_help("the pattern may need to be quoted depending on shell\n\n".to_string() + IN_HELP)
    );
    main_cmd = main_cmd.subcommand(
        Command::new("completions")
            .arg(
                Arg::new("shell").short('s').long("shell").help("shell target").value_name("NAME")
                    .required(true)
                    .value_parser(["bash","elv","fish","ps1","zsh"])
            )
            .about("write completions script to stdout for the specified shell")
    );
    return main_cmd;
}
