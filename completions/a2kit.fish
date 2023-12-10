complete -c a2kit -n "__fish_use_subcommand" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c a2kit -n "__fish_use_subcommand" -s V -l version -d 'Print version'
complete -c a2kit -n "__fish_use_subcommand" -f -a "mkdsk" -d 'write a blank disk image to the given path'
complete -c a2kit -n "__fish_use_subcommand" -f -a "mkdir" -d 'create a new directory inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "delete" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "protect" -d 'password protect a disk or file'
complete -c a2kit -n "__fish_use_subcommand" -f -a "unprotect" -d 'remove password protection from a disk or file'
complete -c a2kit -n "__fish_use_subcommand" -f -a "lock" -d 'write protect a file or directory inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "unlock" -d 'remove write protection from a file or directory inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "rename" -d 'rename a file or directory inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "retype" -d 'change file type inside a disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "verify" -d 'read from stdin and error check'
complete -c a2kit -n "__fish_use_subcommand" -f -a "minify" -d 'reduce program size'
complete -c a2kit -n "__fish_use_subcommand" -f -a "renumber" -d 'renumber BASIC program lines'
complete -c a2kit -n "__fish_use_subcommand" -f -a "get" -d 'read from stdin, local, or disk image, write to stdout'
complete -c a2kit -n "__fish_use_subcommand" -f -a "put" -d 'read from stdin, write to local or disk image'
complete -c a2kit -n "__fish_use_subcommand" -f -a "catalog" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_use_subcommand" -f -a "tree" -d 'write directory tree as a JSON string to stdout'
complete -c a2kit -n "__fish_use_subcommand" -f -a "tokenize" -d 'read from stdin, tokenize, write to stdout'
complete -c a2kit -n "__fish_use_subcommand" -f -a "detokenize" -d 'read from stdin, detokenize, write to stdout'
complete -c a2kit -n "__fish_use_subcommand" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s v -l volume -d 'volume name or number' -r
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s t -l type -d 'type of disk image to create' -r -f -a "{d13	'',do	'',po	'',woz1	'',woz2	'',imd	'',img	'',2mg	'',nib	'',td0	''}"
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s o -l os -d 'operating system format' -r -f -a "{cpm2	'',cpm3	'',dos32	'',dos33	'',prodos	'',pascal	'',fat	''}"
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s k -l kind -d 'kind of disk' -r -f -a "{8in	'',8in-trs80	'',8in-nabu	'',5.25in	'',5.25in-ibm-ssdd8	'',5.25in-ibm-ssdd9	'',5.25in-ibm-dsdd8	'',5.25in-ibm-dsdd9	'',5.25in-ibm-ssqd	'',5.25in-ibm-dsqd	'',5.25in-ibm-dshd	'',5.25in-kayii	'',5.25in-kay4	'',5.25in-osb-sd	'',5.25in-osb-dd	'',3.5in	'',3.5in-ss	'',3.5in-ds	'',3.5in-ibm-720	'',3.5in-ibm-1440	'',3.5in-ibm-2880	'',3in-amstrad	'',hdmax	''}"
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s d -l dimg -d 'disk image path to create' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s w -l wrap -d 'type of disk image to wrap' -r -f -a "{do	'',po	'',nib	''}"
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s b -l bootable -d 'make disk bootable'
complete -c a2kit -n "__fish_seen_subcommand_from mkdsk" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from mkdir" -s f -l file -d 'path inside disk image of new directory' -r
complete -c a2kit -n "__fish_seen_subcommand_from mkdir" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from mkdir" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from delete" -s f -l file -d 'path inside disk image to delete' -r
complete -c a2kit -n "__fish_seen_subcommand_from delete" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from delete" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from protect" -s f -l file -d 'path inside disk image to protect' -r
complete -c a2kit -n "__fish_seen_subcommand_from protect" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from protect" -s p -l password -d 'password to assign' -r
complete -c a2kit -n "__fish_seen_subcommand_from protect" -l read -d 'protect read'
complete -c a2kit -n "__fish_seen_subcommand_from protect" -l write -d 'protect read'
complete -c a2kit -n "__fish_seen_subcommand_from protect" -l delete -d 'protect read'
complete -c a2kit -n "__fish_seen_subcommand_from protect" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from unprotect" -s f -l file -d 'path inside disk image to unprotect' -r
complete -c a2kit -n "__fish_seen_subcommand_from unprotect" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from unprotect" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from lock" -s f -l file -d 'path inside disk image to lock' -r
complete -c a2kit -n "__fish_seen_subcommand_from lock" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from lock" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from unlock" -s f -l file -d 'path inside disk image to unlock' -r
complete -c a2kit -n "__fish_seen_subcommand_from unlock" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from unlock" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from rename" -s f -l file -d 'path inside disk image to rename' -r
complete -c a2kit -n "__fish_seen_subcommand_from rename" -s n -l name -d 'new name' -r
complete -c a2kit -n "__fish_seen_subcommand_from rename" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from rename" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from retype" -s f -l file -d 'path inside disk image to retype' -r
complete -c a2kit -n "__fish_seen_subcommand_from retype" -s t -l type -d 'file system type, code or mnemonic' -r
complete -c a2kit -n "__fish_seen_subcommand_from retype" -s a -l aux -d 'file system auxiliary metadata' -r
complete -c a2kit -n "__fish_seen_subcommand_from retype" -s d -l dimg -d 'path to disk image itself' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from retype" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from verify" -s t -l type -d 'type of the file' -r -f -a "{atxt	'',itxt	'',mtxt	''}"
complete -c a2kit -n "__fish_seen_subcommand_from verify" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from minify" -s t -l type -d 'type of the file' -r -f -a "{atxt	''}"
complete -c a2kit -n "__fish_seen_subcommand_from minify" -l level -d 'set minification level' -r -f -a "{0	'',1	'',2	'',3	''}"
complete -c a2kit -n "__fish_seen_subcommand_from minify" -l flags -d 'set minification flags' -r
complete -c a2kit -n "__fish_seen_subcommand_from minify" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s t -l type -d 'type of the file' -r -f -a "{atxt	''}"
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s b -l beg -d 'lowest number to renumber' -r
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s e -l end -d 'highest number to renumber plus 1' -r
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s f -l first -d 'first number' -r
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s s -l step -d 'step between numbers' -r
complete -c a2kit -n "__fish_seen_subcommand_from renumber" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from get" -s f -l file -d 'path, key, or address, maybe inside disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from get" -s t -l type -d 'type of the item' -r -f -a "{any	'',bin	'',txt	'',raw	'',rec	'',atok	'',itok	'',mtok	'',block	'',sec	'',track	'',raw_track	'',meta	''}"
complete -c a2kit -n "__fish_seen_subcommand_from get" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from get" -s l -l len -d 'length of record in DOS 3.3 random access text file' -r
complete -c a2kit -n "__fish_seen_subcommand_from get" -l trunc -d 'truncate raw at EOF if possible'
complete -c a2kit -n "__fish_seen_subcommand_from get" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from put" -s f -l file -d 'path, key, or address, maybe inside disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from put" -s t -l type -d 'type of the item' -r -f -a "{any	'',bin	'',txt	'',raw	'',rec	'',atok	'',itok	'',mtok	'',block	'',sec	'',track	'',raw_track	'',meta	''}"
complete -c a2kit -n "__fish_seen_subcommand_from put" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from put" -s a -l addr -d 'address of binary file' -r
complete -c a2kit -n "__fish_seen_subcommand_from put" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from catalog" -s f -l file -d 'path of directory inside disk image' -r
complete -c a2kit -n "__fish_seen_subcommand_from catalog" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from catalog" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from tree" -s d -l dimg -d 'path to disk image' -r -F
complete -c a2kit -n "__fish_seen_subcommand_from tree" -l meta -d 'include metadata'
complete -c a2kit -n "__fish_seen_subcommand_from tree" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from tokenize" -s a -l addr -d 'address of tokenized code (Applesoft only)' -r
complete -c a2kit -n "__fish_seen_subcommand_from tokenize" -s t -l type -d 'type of the file' -r -f -a "{atxt	'',itxt	'',mtxt	''}"
complete -c a2kit -n "__fish_seen_subcommand_from tokenize" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from detokenize" -s t -l type -d 'type of the file' -r -f -a "{atok	'',itok	'',mtok	''}"
complete -c a2kit -n "__fish_seen_subcommand_from detokenize" -s h -l help -d 'Print help'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "mkdsk" -d 'write a blank disk image to the given path'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "mkdir" -d 'create a new directory inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "delete" -d 'delete a file or directory inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "protect" -d 'password protect a disk or file'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "unprotect" -d 'remove password protection from a disk or file'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "lock" -d 'write protect a file or directory inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "unlock" -d 'remove write protection from a file or directory inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "rename" -d 'rename a file or directory inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "retype" -d 'change file type inside a disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "verify" -d 'read from stdin and error check'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "minify" -d 'reduce program size'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "renumber" -d 'renumber BASIC program lines'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "get" -d 'read from stdin, local, or disk image, write to stdout'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "put" -d 'read from stdin, write to local or disk image'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "catalog" -d 'write disk image catalog to stdout'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "tree" -d 'write directory tree as a JSON string to stdout'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "tokenize" -d 'read from stdin, tokenize, write to stdout'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "detokenize" -d 'read from stdin, detokenize, write to stdout'
complete -c a2kit -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from mkdsk; and not __fish_seen_subcommand_from mkdir; and not __fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from protect; and not __fish_seen_subcommand_from unprotect; and not __fish_seen_subcommand_from lock; and not __fish_seen_subcommand_from unlock; and not __fish_seen_subcommand_from rename; and not __fish_seen_subcommand_from retype; and not __fish_seen_subcommand_from verify; and not __fish_seen_subcommand_from minify; and not __fish_seen_subcommand_from renumber; and not __fish_seen_subcommand_from get; and not __fish_seen_subcommand_from put; and not __fish_seen_subcommand_from catalog; and not __fish_seen_subcommand_from tree; and not __fish_seen_subcommand_from tokenize; and not __fish_seen_subcommand_from detokenize; and not __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'