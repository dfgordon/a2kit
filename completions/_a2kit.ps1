
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'a2kit' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'a2kit'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'a2kit' {
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', 'V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('mkdsk', 'mkdsk', [CompletionResultType]::ParameterValue, 'write a blank disk image to the given path')
            [CompletionResult]::new('mkdir', 'mkdir', [CompletionResultType]::ParameterValue, 'create a new directory inside a disk image')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'delete a file or directory inside a disk image')
            [CompletionResult]::new('del', 'del', [CompletionResultType]::ParameterValue, 'delete a file or directory inside a disk image')
            [CompletionResult]::new('era', 'era', [CompletionResultType]::ParameterValue, 'delete a file or directory inside a disk image')
            [CompletionResult]::new('protect', 'protect', [CompletionResultType]::ParameterValue, 'password protect a disk or file')
            [CompletionResult]::new('unprotect', 'unprotect', [CompletionResultType]::ParameterValue, 'remove password protection from a disk or file')
            [CompletionResult]::new('lock', 'lock', [CompletionResultType]::ParameterValue, 'write protect a file or directory inside a disk image')
            [CompletionResult]::new('unlock', 'unlock', [CompletionResultType]::ParameterValue, 'remove write protection from a file or directory inside a disk image')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'rename a file or directory inside a disk image')
            [CompletionResult]::new('retype', 'retype', [CompletionResultType]::ParameterValue, 'change file type inside a disk image')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'read from stdin and error check')
            [CompletionResult]::new('minify', 'minify', [CompletionResultType]::ParameterValue, 'reduce program size')
            [CompletionResult]::new('renumber', 'renumber', [CompletionResultType]::ParameterValue, 'renumber BASIC program lines')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'read from stdin, local, or disk image, write to stdout')
            [CompletionResult]::new('put', 'put', [CompletionResultType]::ParameterValue, 'read from stdin, write to local or disk image')
            [CompletionResult]::new('catalog', 'catalog', [CompletionResultType]::ParameterValue, 'write disk image catalog to stdout')
            [CompletionResult]::new('cat', 'cat', [CompletionResultType]::ParameterValue, 'write disk image catalog to stdout')
            [CompletionResult]::new('dir', 'dir', [CompletionResultType]::ParameterValue, 'write disk image catalog to stdout')
            [CompletionResult]::new('ls', 'ls', [CompletionResultType]::ParameterValue, 'write disk image catalog to stdout')
            [CompletionResult]::new('tree', 'tree', [CompletionResultType]::ParameterValue, 'write directory tree as a JSON string to stdout')
            [CompletionResult]::new('stat', 'stat', [CompletionResultType]::ParameterValue, 'write FS statistics as a JSON string to stdout')
            [CompletionResult]::new('geometry', 'geometry', [CompletionResultType]::ParameterValue, 'write disk geometry as a JSON string to stdout')
            [CompletionResult]::new('tokenize', 'tokenize', [CompletionResultType]::ParameterValue, 'read from stdin, tokenize, write to stdout')
            [CompletionResult]::new('tok', 'tok', [CompletionResultType]::ParameterValue, 'read from stdin, tokenize, write to stdout')
            [CompletionResult]::new('detokenize', 'detokenize', [CompletionResultType]::ParameterValue, 'read from stdin, detokenize, write to stdout')
            [CompletionResult]::new('dtok', 'dtok', [CompletionResultType]::ParameterValue, 'read from stdin, detokenize, write to stdout')
            [CompletionResult]::new('asm', 'asm', [CompletionResultType]::ParameterValue, 'read from stdin, assemble, write to stdout')
            [CompletionResult]::new('dasm', 'dasm', [CompletionResultType]::ParameterValue, 'read from stdin, disassemble, write to stdout')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'a2kit;mkdsk' {
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'volume name or number')
            [CompletionResult]::new('--volume', 'volume', [CompletionResultType]::ParameterName, 'volume name or number')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of disk image to create')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of disk image to create')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'operating system format')
            [CompletionResult]::new('--os', 'os', [CompletionResultType]::ParameterName, 'operating system format')
            [CompletionResult]::new('-k', 'k', [CompletionResultType]::ParameterName, 'kind of disk')
            [CompletionResult]::new('--kind', 'kind', [CompletionResultType]::ParameterName, 'kind of disk')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'disk image path to create')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'disk image path to create')
            [CompletionResult]::new('-w', 'w', [CompletionResultType]::ParameterName, 'type of disk image to wrap')
            [CompletionResult]::new('--wrap', 'wrap', [CompletionResultType]::ParameterName, 'type of disk image to wrap')
            [CompletionResult]::new('-b', 'b', [CompletionResultType]::ParameterName, 'make disk bootable')
            [CompletionResult]::new('--bootable', 'bootable', [CompletionResultType]::ParameterName, 'make disk bootable')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;mkdir' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image of new directory')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image of new directory')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;delete' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;del' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;era' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to delete')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;protect' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to protect')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to protect')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'password to assign')
            [CompletionResult]::new('--password', 'password', [CompletionResultType]::ParameterName, 'password to assign')
            [CompletionResult]::new('--read', 'read', [CompletionResultType]::ParameterName, 'protect read')
            [CompletionResult]::new('--write', 'write', [CompletionResultType]::ParameterName, 'protect read')
            [CompletionResult]::new('--delete', 'delete', [CompletionResultType]::ParameterName, 'protect read')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;unprotect' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to unprotect')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to unprotect')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;lock' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to lock')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to lock')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;unlock' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to unlock')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to unlock')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;rename' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to rename')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to rename')
            [CompletionResult]::new('-n', 'n', [CompletionResultType]::ParameterName, 'new name')
            [CompletionResult]::new('--name', 'name', [CompletionResultType]::ParameterName, 'new name')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;retype' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path inside disk image to retype')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path inside disk image to retype')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'file system type, code or mnemonic')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'file system type, code or mnemonic')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'file system auxiliary metadata')
            [CompletionResult]::new('--aux', 'aux', [CompletionResultType]::ParameterName, 'file system auxiliary metadata')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image itself')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;verify' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-c', 'c', [CompletionResultType]::ParameterName, 'modify diagnostic configuration')
            [CompletionResult]::new('--config', 'config', [CompletionResultType]::ParameterName, 'modify diagnostic configuration')
            [CompletionResult]::new('-w', 'w', [CompletionResultType]::ParameterName, 'workspace directory')
            [CompletionResult]::new('--workspace', 'workspace', [CompletionResultType]::ParameterName, 'workspace directory')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'write S-expressions to stderr')
            [CompletionResult]::new('--sexpr', 'sexpr', [CompletionResultType]::ParameterName, 'write S-expressions to stderr')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;minify' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--level', 'level', [CompletionResultType]::ParameterName, 'set minification level')
            [CompletionResult]::new('--flags', 'flags', [CompletionResultType]::ParameterName, 'set minification flags')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;renumber' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-b', 'b', [CompletionResultType]::ParameterName, 'lowest number to renumber')
            [CompletionResult]::new('--beg', 'beg', [CompletionResultType]::ParameterName, 'lowest number to renumber')
            [CompletionResult]::new('-e', 'e', [CompletionResultType]::ParameterName, 'highest number to renumber plus 1')
            [CompletionResult]::new('--end', 'end', [CompletionResultType]::ParameterName, 'highest number to renumber plus 1')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'first number')
            [CompletionResult]::new('--first', 'first', [CompletionResultType]::ParameterName, 'first number')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'step between numbers')
            [CompletionResult]::new('--step', 'step', [CompletionResultType]::ParameterName, 'step between numbers')
            [CompletionResult]::new('-r', 'r', [CompletionResultType]::ParameterName, 'allow reordering of lines')
            [CompletionResult]::new('--reorder', 'reorder', [CompletionResultType]::ParameterName, 'allow reordering of lines')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;get' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path, key, or address, maybe inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path, key, or address, maybe inside disk image')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the item')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the item')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'length of record in DOS 3.3 random access text file')
            [CompletionResult]::new('--len', 'len', [CompletionResultType]::ParameterName, 'length of record in DOS 3.3 random access text file')
            [CompletionResult]::new('--trunc', 'trunc', [CompletionResultType]::ParameterName, 'truncate raw at EOF if possible')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;put' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path, key, or address, maybe inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path, key, or address, maybe inside disk image')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the item')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the item')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'address of binary file')
            [CompletionResult]::new('--addr', 'addr', [CompletionResultType]::ParameterName, 'address of binary file')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;catalog' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--generic', 'generic', [CompletionResultType]::ParameterName, 'use generic output format')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;cat' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--generic', 'generic', [CompletionResultType]::ParameterName, 'use generic output format')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;dir' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--generic', 'generic', [CompletionResultType]::ParameterName, 'use generic output format')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;ls' {
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('--file', 'file', [CompletionResultType]::ParameterName, 'path of directory inside disk image')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--generic', 'generic', [CompletionResultType]::ParameterName, 'use generic output format')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;tree' {
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--meta', 'meta', [CompletionResultType]::ParameterName, 'include metadata')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;stat' {
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;geometry' {
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('--dimg', 'dimg', [CompletionResultType]::ParameterName, 'path to disk image')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;tokenize' {
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'address of tokenized code (Applesoft only)')
            [CompletionResult]::new('--addr', 'addr', [CompletionResultType]::ParameterName, 'address of tokenized code (Applesoft only)')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;tok' {
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'address of tokenized code (Applesoft only)')
            [CompletionResult]::new('--addr', 'addr', [CompletionResultType]::ParameterName, 'address of tokenized code (Applesoft only)')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;detokenize' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;dtok' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('--type', 'type', [CompletionResultType]::ParameterName, 'type of the file')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;asm' {
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'assembler variant')
            [CompletionResult]::new('--assembler', 'assembler', [CompletionResultType]::ParameterName, 'assembler variant')
            [CompletionResult]::new('-w', 'w', [CompletionResultType]::ParameterName, 'workspace directory')
            [CompletionResult]::new('--workspace', 'workspace', [CompletionResultType]::ParameterName, 'workspace directory')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;dasm' {
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'processor target')
            [CompletionResult]::new('--proc', 'proc', [CompletionResultType]::ParameterName, 'processor target')
            [CompletionResult]::new('--mx', 'mx', [CompletionResultType]::ParameterName, 'MX status bits')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'starting address')
            [CompletionResult]::new('--org', 'org', [CompletionResultType]::ParameterName, 'starting address')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'a2kit;help' {
            [CompletionResult]::new('mkdsk', 'mkdsk', [CompletionResultType]::ParameterValue, 'write a blank disk image to the given path')
            [CompletionResult]::new('mkdir', 'mkdir', [CompletionResultType]::ParameterValue, 'create a new directory inside a disk image')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'delete a file or directory inside a disk image')
            [CompletionResult]::new('protect', 'protect', [CompletionResultType]::ParameterValue, 'password protect a disk or file')
            [CompletionResult]::new('unprotect', 'unprotect', [CompletionResultType]::ParameterValue, 'remove password protection from a disk or file')
            [CompletionResult]::new('lock', 'lock', [CompletionResultType]::ParameterValue, 'write protect a file or directory inside a disk image')
            [CompletionResult]::new('unlock', 'unlock', [CompletionResultType]::ParameterValue, 'remove write protection from a file or directory inside a disk image')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'rename a file or directory inside a disk image')
            [CompletionResult]::new('retype', 'retype', [CompletionResultType]::ParameterValue, 'change file type inside a disk image')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'read from stdin and error check')
            [CompletionResult]::new('minify', 'minify', [CompletionResultType]::ParameterValue, 'reduce program size')
            [CompletionResult]::new('renumber', 'renumber', [CompletionResultType]::ParameterValue, 'renumber BASIC program lines')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'read from stdin, local, or disk image, write to stdout')
            [CompletionResult]::new('put', 'put', [CompletionResultType]::ParameterValue, 'read from stdin, write to local or disk image')
            [CompletionResult]::new('catalog', 'catalog', [CompletionResultType]::ParameterValue, 'write disk image catalog to stdout')
            [CompletionResult]::new('tree', 'tree', [CompletionResultType]::ParameterValue, 'write directory tree as a JSON string to stdout')
            [CompletionResult]::new('stat', 'stat', [CompletionResultType]::ParameterValue, 'write FS statistics as a JSON string to stdout')
            [CompletionResult]::new('geometry', 'geometry', [CompletionResultType]::ParameterValue, 'write disk geometry as a JSON string to stdout')
            [CompletionResult]::new('tokenize', 'tokenize', [CompletionResultType]::ParameterValue, 'read from stdin, tokenize, write to stdout')
            [CompletionResult]::new('detokenize', 'detokenize', [CompletionResultType]::ParameterValue, 'read from stdin, detokenize, write to stdout')
            [CompletionResult]::new('asm', 'asm', [CompletionResultType]::ParameterValue, 'read from stdin, assemble, write to stdout')
            [CompletionResult]::new('dasm', 'dasm', [CompletionResultType]::ParameterValue, 'read from stdin, disassemble, write to stdout')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'a2kit;help;mkdsk' {
            break
        }
        'a2kit;help;mkdir' {
            break
        }
        'a2kit;help;delete' {
            break
        }
        'a2kit;help;protect' {
            break
        }
        'a2kit;help;unprotect' {
            break
        }
        'a2kit;help;lock' {
            break
        }
        'a2kit;help;unlock' {
            break
        }
        'a2kit;help;rename' {
            break
        }
        'a2kit;help;retype' {
            break
        }
        'a2kit;help;verify' {
            break
        }
        'a2kit;help;minify' {
            break
        }
        'a2kit;help;renumber' {
            break
        }
        'a2kit;help;get' {
            break
        }
        'a2kit;help;put' {
            break
        }
        'a2kit;help;catalog' {
            break
        }
        'a2kit;help;tree' {
            break
        }
        'a2kit;help;stat' {
            break
        }
        'a2kit;help;geometry' {
            break
        }
        'a2kit;help;tokenize' {
            break
        }
        'a2kit;help;detokenize' {
            break
        }
        'a2kit;help;asm' {
            break
        }
        'a2kit;help;dasm' {
            break
        }
        'a2kit;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
