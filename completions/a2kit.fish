# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_a2kit_global_optspecs
	string join \n h/help V/version
end

function __fish_a2kit_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_a2kit_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_a2kit_using_subcommand
	set -l cmd (__fish_a2kit_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c a2kit -n "__fish_a2kit_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c a2kit -n "__fish_a2kit_needs_command" -s V -l version -d 'Print version'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "mkdsk" -d 'write a blank disk image to the given path'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "mkdir" -d 'create a new directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "delete" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "del" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "era" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "protect" -d 'password protect a disk or file'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "unprotect" -d 'remove password protection from a disk or file'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "lock" -d 'write protect a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "unlock" -d 'remove write protection from a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "rename" -d 'rename a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "retype" -d 'change file type inside a disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "verify" -d 'read from stdin and error check'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "minify" -d 'reduce program size'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "renumber" -d 'renumber BASIC program lines'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "get" -d 'read from stdin, local, or disk image, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "put" -d 'read from stdin, write to local or disk image'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "catalog" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "cat" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "dir" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "ls" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "tree" -d 'write directory tree as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "stat" -d 'write FS statistics as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "geometry" -d 'write disk geometry as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "tokenize" -d 'read from stdin, tokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "tok" -d 'read from stdin, tokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "detokenize" -d 'read from stdin, detokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "dtok" -d 'read from stdin, detokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "asm" -d 'read from stdin, assemble, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "dasm" -d 'read from stdin, disassemble, write to stdout'
complete -c a2kit -n "__fish_a2kit_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s v -l volume -d 'volume name or number' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s t -l type -d 'type of disk image to create' -r -f -a "{d13\t'',do\t'',po\t'',woz1\t'',woz2\t'',imd\t'',img\t'',2mg\t'',nib\t'',td0\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s o -l os -d 'operating system format' -r -f -a "{cpm2\t'',cpm3\t'',dos32\t'',dos33\t'',prodos\t'',pascal\t'',fat\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s k -l kind -d 'kind of disk' -r -f -a "{8in\t'',8in-trs80\t'',8in-nabu\t'',5.25in\t'',5.25in-ibm-ssdd8\t'',5.25in-ibm-ssdd9\t'',5.25in-ibm-dsdd8\t'',5.25in-ibm-dsdd9\t'',5.25in-ibm-ssqd\t'',5.25in-ibm-dsqd\t'',5.25in-ibm-dshd\t'',5.25in-kayii\t'',5.25in-kay4\t'',5.25in-osb-sd\t'',5.25in-osb-dd\t'',3.5in\t'',3.5in-ss\t'',3.5in-ds\t'',3.5in-ibm-720\t'',3.5in-ibm-1440\t'',3.5in-ibm-2880\t'',3in-amstrad\t'',hdmax\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s d -l dimg -d 'disk image path to create' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s w -l wrap -d 'type of disk image to wrap' -r -f -a "{do\t'',po\t'',nib\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s b -l bootable -d 'make disk bootable'
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdsk" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdir" -s f -l file -d 'path inside disk image of new directory' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdir" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand mkdir" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand delete" -s f -l file -d 'path inside disk image to delete' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand delete" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand delete" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand del" -s f -l file -d 'path inside disk image to delete' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand del" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand del" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand era" -s f -l file -d 'path inside disk image to delete' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand era" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand era" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -s f -l file -d 'path inside disk image to protect' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -s p -l password -d 'password to assign' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -l read -d 'protect read'
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -l write -d 'protect read'
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -l delete -d 'protect read'
complete -c a2kit -n "__fish_a2kit_using_subcommand protect" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand unprotect" -s f -l file -d 'path inside disk image to unprotect' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand unprotect" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand unprotect" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand lock" -s f -l file -d 'path inside disk image to lock' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand lock" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand lock" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand unlock" -s f -l file -d 'path inside disk image to unlock' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand unlock" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand unlock" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand rename" -s f -l file -d 'path inside disk image to rename' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand rename" -s n -l name -d 'new name' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand rename" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand rename" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand retype" -s f -l file -d 'path inside disk image to retype' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand retype" -s t -l type -d 'file system type, code or mnemonic' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand retype" -s a -l aux -d 'file system auxiliary metadata' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand retype" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand retype" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand verify" -s t -l type -d 'type of the file' -r -f -a "{atxt\t'',itxt\t'',mtxt\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand verify" -s c -l config -d 'modify diagnostic configuration' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand verify" -s w -l workspace -d 'workspace directory' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand verify" -s s -l sexpr -d 'write S-expressions to stderr'
complete -c a2kit -n "__fish_a2kit_using_subcommand verify" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand minify" -s t -l type -d 'type of the file' -r -f -a "{atxt\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand minify" -l level -d 'set minification level' -r -f -a "{0\t'',1\t'',2\t'',3\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand minify" -l flags -d 'set minification flags' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand minify" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s t -l type -d 'type of the file' -r -f -a "{atxt\t'',itxt\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s b -l beg -d 'lowest number to renumber' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s e -l end -d 'highest number to renumber plus 1' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s f -l first -d 'first number' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s s -l step -d 'step between numbers' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s r -l reorder -d 'allow reordering of lines'
complete -c a2kit -n "__fish_a2kit_using_subcommand renumber" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -s f -l file -d 'path, key, or address, maybe inside disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -s t -l type -d 'type of the item' -r -f -a "{any\t'',auto\t'',bin\t'',txt\t'',raw\t'',rec\t'',atok\t'',itok\t'',mtok\t'',block\t'',sec\t'',track\t'',raw_track\t'',meta\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -s l -l len -d 'length of record in DOS 3.3 random access text file' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -l trunc -d 'truncate raw at EOF if possible'
complete -c a2kit -n "__fish_a2kit_using_subcommand get" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand put" -s f -l file -d 'path, key, or address, maybe inside disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand put" -s t -l type -d 'type of the item' -r -f -a "{any\t'',auto\t'',bin\t'',txt\t'',raw\t'',rec\t'',atok\t'',itok\t'',mtok\t'',block\t'',sec\t'',track\t'',raw_track\t'',meta\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand put" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand put" -s a -l addr -d 'address of binary file' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand put" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand catalog" -s f -l file -d 'path of directory inside disk image' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand catalog" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand catalog" -l generic -d 'use generic output format'
complete -c a2kit -n "__fish_a2kit_using_subcommand catalog" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand cat" -s f -l file -d 'path of directory inside disk image' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand cat" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand cat" -l generic -d 'use generic output format'
complete -c a2kit -n "__fish_a2kit_using_subcommand cat" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand dir" -s f -l file -d 'path of directory inside disk image' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand dir" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand dir" -l generic -d 'use generic output format'
complete -c a2kit -n "__fish_a2kit_using_subcommand dir" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand ls" -s f -l file -d 'path of directory inside disk image' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand ls" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand ls" -l generic -d 'use generic output format'
complete -c a2kit -n "__fish_a2kit_using_subcommand ls" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand tree" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand tree" -l meta -d 'include metadata'
complete -c a2kit -n "__fish_a2kit_using_subcommand tree" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand stat" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand stat" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand geometry" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_a2kit_using_subcommand geometry" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand tokenize" -s a -l addr -d 'address of tokenized code (Applesoft only)' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand tokenize" -s t -l type -d 'type of the file' -r -f -a "{atxt\t'',itxt\t'',mtxt\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand tokenize" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand tok" -s a -l addr -d 'address of tokenized code (Applesoft only)' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand tok" -s t -l type -d 'type of the file' -r -f -a "{atxt\t'',itxt\t'',mtxt\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand tok" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand detokenize" -s t -l type -d 'type of the file' -r -f -a "{atok\t'',itok\t'',mtok\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand detokenize" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand dtok" -s t -l type -d 'type of the file' -r -f -a "{atok\t'',itok\t'',mtok\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand dtok" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand asm" -s a -l assembler -d 'assembler variant' -r -f -a "{m8\t'',m16\t'',m16+\t'',m32\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand asm" -s w -l workspace -d 'workspace directory' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand asm" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand dasm" -s p -l proc -d 'processor target' -r -f -a "{6502\t'',65c02\t'',65802\t'',65816\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand dasm" -l mx -d 'MX status bits' -r -f -a "{00\t'',01\t'',10\t'',11\t''}"
complete -c a2kit -n "__fish_a2kit_using_subcommand dasm" -s o -l org -d 'starting address' -r
complete -c a2kit -n "__fish_a2kit_using_subcommand dasm" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "mkdsk" -d 'write a blank disk image to the given path'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "mkdir" -d 'create a new directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "delete" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "protect" -d 'password protect a disk or file'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "unprotect" -d 'remove password protection from a disk or file'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "lock" -d 'write protect a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "unlock" -d 'remove write protection from a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "rename" -d 'rename a file or directory inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "retype" -d 'change file type inside a disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "verify" -d 'read from stdin and error check'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "minify" -d 'reduce program size'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "renumber" -d 'renumber BASIC program lines'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "get" -d 'read from stdin, local, or disk image, write to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "put" -d 'read from stdin, write to local or disk image'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "catalog" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "tree" -d 'write directory tree as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "stat" -d 'write FS statistics as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "geometry" -d 'write disk geometry as a JSON string to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "tokenize" -d 'read from stdin, tokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "detokenize" -d 'read from stdin, detokenize, write to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "asm" -d 'read from stdin, assemble, write to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "dasm" -d 'read from stdin, disassemble, write to stdout'
complete -c a2kit -n "__fish_a2kit_using_subcommand help; and not __fish_seen_subcommand_from mkdsk mkdir delete protect unprotect lock unlock rename retype verify minify renumber get put catalog tree stat geometry tokenize detokenize asm dasm help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
