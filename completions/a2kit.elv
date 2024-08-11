
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
            cand get 'read from stdin, local, or disk image, write to stdout'
            cand put 'read from stdin, write to local or disk image'
            cand mget 'read list of paths from stdin, get files from disk image, write file images to stdout'
            cand mput 'read list of file images from stdin, restore files to a disk image'
            cand pack 'pack data into a file image'
            cand unpack 'unpack data from a file image'
            cand mkdsk 'write a blank disk image to the given path'
            cand mkdir 'create a new directory inside a disk image'
            cand delete 'delete a file or directory inside a disk image'
            cand del 'delete a file or directory inside a disk image'
            cand era 'delete a file or directory inside a disk image'
            cand protect 'password protect a disk or file'
            cand unprotect 'remove password protection from a disk or file'
            cand lock 'write protect a file or directory inside a disk image'
            cand unlock 'remove write protection from a file or directory inside a disk image'
            cand rename 'rename a file or directory inside a disk image'
            cand retype 'change file type inside a disk image'
            cand verify 'read from stdin and perform language analysis'
            cand minify 'reduce program size'
            cand renumber 'renumber BASIC program lines'
            cand catalog 'write disk image catalog to stdout'
            cand cat 'write disk image catalog to stdout'
            cand dir 'write disk image catalog to stdout'
            cand ls 'write disk image catalog to stdout'
            cand tree 'write directory tree as a JSON string to stdout'
            cand stat 'write FS statistics as a JSON string to stdout'
            cand geometry 'write disk geometry as a JSON string to stdout'
            cand tokenize 'read from stdin, tokenize, write to stdout'
            cand tok 'read from stdin, tokenize, write to stdout'
            cand detokenize 'read from stdin, detokenize, write to stdout'
            cand dtok 'read from stdin, detokenize, write to stdout'
            cand asm 'read from stdin, assemble, write to stdout'
            cand dasm 'read from stdin, disassemble, write to stdout'
            cand glob 'write JSON list of matching paths to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'a2kit;get'= {
            cand -f 'path, key, or address, maybe inside disk image'
            cand --file 'path, key, or address, maybe inside disk image'
            cand -t 'type of the item'
            cand --type 'type of the item'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --indent 'JSON indentation, omit to minify'
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
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -a 'load-address if applicable'
            cand --addr 'load-address if applicable'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;mget'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --indent 'JSON indentation, omit to minify'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;mput'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -f 'override target paths'
            cand --file 'override target paths'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;pack'= {
            cand -f 'target path for this file image'
            cand --file 'target path for this file image'
            cand -t 'type of the item'
            cand --type 'type of the item'
            cand -a 'load-address if applicable'
            cand --addr 'load-address if applicable'
            cand -b 'size of block in bytes if needed'
            cand --block 'size of block in bytes if needed'
            cand -o 'operating system format'
            cand --os 'operating system format'
            cand --indent 'JSON indentation, omit to minify'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;unpack'= {
            cand -t 'type of the item'
            cand --type 'type of the item'
            cand -l 'length of record in DOS 3.3 random access text file'
            cand --len 'length of record in DOS 3.3 random access text file'
            cand --trunc 'truncate raw at EOF if possible'
            cand -h 'Print help'
            cand --help 'Print help'
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
        &'a2kit;del'= {
            cand -f 'path inside disk image to delete'
            cand --file 'path inside disk image to delete'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;era'= {
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
            cand -c 'modify diagnostic configuration'
            cand --config 'modify diagnostic configuration'
            cand -w 'workspace directory'
            cand --workspace 'workspace directory'
            cand -s 'write S-expressions to stderr'
            cand --sexpr 'write S-expressions to stderr'
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
            cand -r 'allow reordering of lines'
            cand --reorder 'allow reordering of lines'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;catalog'= {
            cand -f 'path of directory inside disk image'
            cand --file 'path of directory inside disk image'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --generic 'use generic output format'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;cat'= {
            cand -f 'path of directory inside disk image'
            cand --file 'path of directory inside disk image'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --generic 'use generic output format'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;dir'= {
            cand -f 'path of directory inside disk image'
            cand --file 'path of directory inside disk image'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --generic 'use generic output format'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;ls'= {
            cand -f 'path of directory inside disk image'
            cand --file 'path of directory inside disk image'
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --generic 'use generic output format'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;tree'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --indent 'JSON indentation, omit to minify'
            cand --meta 'include metadata'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;stat'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --indent 'JSON indentation, omit to minify'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;geometry'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand --indent 'JSON indentation, omit to minify'
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
        &'a2kit;tok'= {
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
        &'a2kit;dtok'= {
            cand -t 'type of the file'
            cand --type 'type of the file'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;asm'= {
            cand -a 'assembler variant'
            cand --assembler 'assembler variant'
            cand -w 'workspace directory'
            cand --workspace 'workspace directory'
            cand --literals 'assign values to disassembled hex labels'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;dasm'= {
            cand -p 'processor target'
            cand --proc 'processor target'
            cand --mx 'MX status bits'
            cand -o 'starting address'
            cand --org 'starting address'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;glob'= {
            cand -d 'path to disk image itself'
            cand --dimg 'path to disk image itself'
            cand -f 'glob pattern to match against'
            cand --file 'glob pattern to match against'
            cand --indent 'JSON indentation, omit to minify'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'a2kit;help'= {
            cand get 'read from stdin, local, or disk image, write to stdout'
            cand put 'read from stdin, write to local or disk image'
            cand mget 'read list of paths from stdin, get files from disk image, write file images to stdout'
            cand mput 'read list of file images from stdin, restore files to a disk image'
            cand pack 'pack data into a file image'
            cand unpack 'unpack data from a file image'
            cand mkdsk 'write a blank disk image to the given path'
            cand mkdir 'create a new directory inside a disk image'
            cand delete 'delete a file or directory inside a disk image'
            cand protect 'password protect a disk or file'
            cand unprotect 'remove password protection from a disk or file'
            cand lock 'write protect a file or directory inside a disk image'
            cand unlock 'remove write protection from a file or directory inside a disk image'
            cand rename 'rename a file or directory inside a disk image'
            cand retype 'change file type inside a disk image'
            cand verify 'read from stdin and perform language analysis'
            cand minify 'reduce program size'
            cand renumber 'renumber BASIC program lines'
            cand catalog 'write disk image catalog to stdout'
            cand tree 'write directory tree as a JSON string to stdout'
            cand stat 'write FS statistics as a JSON string to stdout'
            cand geometry 'write disk geometry as a JSON string to stdout'
            cand tokenize 'read from stdin, tokenize, write to stdout'
            cand detokenize 'read from stdin, detokenize, write to stdout'
            cand asm 'read from stdin, assemble, write to stdout'
            cand dasm 'read from stdin, disassemble, write to stdout'
            cand glob 'write JSON list of matching paths to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'a2kit;help;get'= {
        }
        &'a2kit;help;put'= {
        }
        &'a2kit;help;mget'= {
        }
        &'a2kit;help;mput'= {
        }
        &'a2kit;help;pack'= {
        }
        &'a2kit;help;unpack'= {
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
        &'a2kit;help;catalog'= {
        }
        &'a2kit;help;tree'= {
        }
        &'a2kit;help;stat'= {
        }
        &'a2kit;help;geometry'= {
        }
        &'a2kit;help;tokenize'= {
        }
        &'a2kit;help;detokenize'= {
        }
        &'a2kit;help;asm'= {
        }
        &'a2kit;help;dasm'= {
        }
        &'a2kit;help;glob'= {
        }
        &'a2kit;help;help'= {
        }
    ]
    $completions[$command]
}
