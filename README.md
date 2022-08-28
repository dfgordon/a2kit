Apple ][ Kit (a2kit)
====================

This project aims to provide a [rust](https://rust-lang.org)-based command line tool that manipulates Apple ][ language files and disk images.  Running `a2kit --help` reveals the subcommands and further help.

The tool is intended to be used with redirection and pipelines.  A few examples follow.

* Create a disk image

    - `a2kit create -v 254 -t do > myblank.do`

* Catalog a disk image

    - `a2kit catalog < img.do`

* Take an Applesoft BASIC source file, tokenize it, and save it as a binary

    - `a2kit tokenize -t atxt -a 2049 < hello.bas > hello.atok`

* Take an Integer BASIC source file, tokenize it, and add it to a disk image

    - `a2kit tokenize -t itxt < hello.bas | put -f HELLO -t itok -d img.do`

* Take a binary file and add it to a disk image

    - `a2kit put -f MYBIN -t bin -d img.do -a 768 < mybin`

* Enter an Applesoft BASIC program in the console, error check it, and display the tokenized program as a hex dump

    - `a2kit verify -t atxt | a2kit tokenize -t atxt -a 2049`

Status
------

In early stages, no release yet.  If you would like to try it you will have to clone both this project and `a2kit_macro`.  Then use [cargo](https://doc.rust-lang.org/cargo/index.html) to build it.

