
use builtin;
use str;

set edit:completion:arg-completer[a2kit] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'a2kit'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'a2kit'= {
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
            cand mkdsk 'write a blank disk image to the given path'
            cand mkdir 'create a new directory inside a disk image'
            cand delete 'delete a file or directory inside a disk image'
            cand protect 'password protect a disk or file'
            cand unprotect 'remove password protection from a disk or file'
            cand lock 'write protect a file or directory inside a disk image'
            cand unlock 'remove write protection from a file or directory inside a disk image'
            cand rename 'rename a file or directory inside a disk image'
            cand retype 'change file type inside a disk image'
            cand verify 'read from stdin and error check'
            cand minify 'reduce program size'
            cand renumber 'renumber BASIC program lines'
            cand get 'read from stdin, local, or disk image, write to stdout'
            cand put 'read from stdin, write to local or disk image'
            cand catalog 'write disk image catalog to stdout'
            cand tree 'write directory tree as a JSON string to stdout'
            cand tokenize 'read from stdin, tokenize, write to stdout'
            cand detokenize 'read from stdin, detokenize, write to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'a2kit;mkdsk'= {
            cand -v 'volume name or number'
            cand --volume 'volume name or number'
            cand -t 'type of disk image to create'
            cand --type 'type of disk image to create'
            cand -o 'operating system format'
            cand --os 'operating system format'
            cand -k 'kind of disk'
            cand --kind 'kind of disk'
            cand -d 'disk image path to create'
            cand --dimg 'disk image path to create'
            cand -w 'type of disk image to wrap'
            cand --wrap 'type of disk image to wrap'
            cand -b 'make disk bootable'
            cand --bootable 'make disk bootable'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;mkdir'= {
            cand -f 'path inside disk image of new directory'
            cand --file 'path inside disk image of new directory'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;delete'= {
            cand -f 'path inside disk image to delete'
            cand --file 'path inside disk image to delete'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;protect'= {
            cand -f 'path inside disk image to protect'
            cand --file 'path inside disk image to protect'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -p 'password to assign'
            cand --password 'password to assign'
            cand --read 'protect read'
            cand --write 'protect read'
            cand --delete 'protect read'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;unprotect'= {
            cand -f 'path inside disk image to unprotect'
            cand --file 'path inside disk image to unprotect'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;lock'= {
            cand -f 'path inside disk image to lock'
            cand --file 'path inside disk image to lock'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;unlock'= {
            cand -f 'path inside disk image to unlock'
            cand --file 'path inside disk image to unlock'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;rename'= {
            cand -f 'path inside disk image to rename'
            cand --file 'path inside disk image to rename'
            cand -n 'new name'
            cand --name 'new name'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;retype'= {
            cand -f 'path inside disk image to retype'
            cand --file 'path inside disk image to retype'
            cand -t 'file system type, code or mnemonic'
            cand --type 'file system type, code or mnemonic'
            cand -a 'file system auxiliary metadata'
            cand --aux 'file system auxiliary metadata'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;verify'= {
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;minify'= {
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand --level 'set minification level'
            cand --flags 'set minification flags'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;renumber'= {
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand -b 'lowest number to renumber'
            cand --beg 'lowest number to renumber'
            cand -e 'highest number to renumber plus 1'
            cand --end 'highest number to renumber plus 1'
            cand -f 'first number'
            cand --first 'first number'
            cand -s 'step between numbers'
            cand --step 'step between numbers'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;get'= {
            cand -f 'path, key, or address, maybe inside disk image'
            cand --file 'path, key, or address, maybe inside disk image'
            cand -t 'type of the item'
            cand --type 'type of the item'
            cand -d 'path to disk image'
            cand --dimg 'path to disk image'
            cand -l 'length of record in DOS 3.3 random access text file'
            cand --len 'length of record in DOS 3.3 random access text file'
            cand --trunc 'truncate raw at EOF if possible'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;put'= {
            cand -f 'path, key, or address, maybe inside disk image'
            cand --file 'path, key, or address, maybe inside disk image'
            cand -t 'type of the item'
            cand --type 'type of the item'
            cand -d 'path to disk image'
            cand --dimg 'path to disk image'
            cand -a 'address of binary file'
            cand --addr 'address of binary file'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;catalog'= {
            cand -f 'path of directory inside disk image'
            cand --file 'path of directory inside disk image'
            cand -d 'path to disk image'
            cand --dimg 'path to disk image'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;tree'= {
            cand -d 'path to disk image'
            cand --dimg 'path to disk image'
            cand --meta 'include metadata'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;tokenize'= {
            cand -a 'address of tokenized code (Applesoft only)'
            cand --addr 'address of tokenized code (Applesoft only)'
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;detokenize'= {
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;help'= {
            cand mkdsk 'write a blank disk image to the given path'
            cand mkdir 'create a new directory inside a disk image'
            cand delete 'delete a file or directory inside a disk image'
            cand protect 'password protect a disk or file'
            cand unprotect 'remove password protection from a disk or file'
            cand lock 'write protect a file or directory inside a disk image'
            cand unlock 'remove write protection from a file or directory inside a disk image'
            cand rename 'rename a file or directory inside a disk image'
            cand retype 'change file type inside a disk image'
            cand verify 'read from stdin and error check'
            cand minify 'reduce program size'
            cand renumber 'renumber BASIC program lines'
            cand get 'read from stdin, local, or disk image, write to stdout'
            cand put 'read from stdin, write to local or disk image'
            cand catalog 'write disk image catalog to stdout'
            cand tree 'write directory tree as a JSON string to stdout'
            cand tokenize 'read from stdin, tokenize, write to stdout'
            cand detokenize 'read from stdin, detokenize, write to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'a2kit;help;mkdsk'= {
        }
        &'a2kit;help;mkdir'= {
        }
        &'a2kit;help;delete'= {
        }
        &'a2kit;help;protect'= {
        }
        &'a2kit;help;unprotect'= {
        }
        &'a2kit;help;lock'= {
        }
        &'a2kit;help;unlock'= {
        }
        &'a2kit;help;rename'= {
        }
        &'a2kit;help;retype'= {
        }
        &'a2kit;help;verify'= {
        }
        &'a2kit;help;minify'= {
        }
        &'a2kit;help;renumber'= {
        }
        &'a2kit;help;get'= {
        }
        &'a2kit;help;put'= {
        }
        &'a2kit;help;catalog'= {
        }
        &'a2kit;help;tree'= {
        }
        &'a2kit;help;tokenize'= {
        }
        &'a2kit;help;detokenize'= {
        }
        &'a2kit;help;help'= {
        }
    ]
    $completions[$command]
}
