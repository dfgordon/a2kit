* Upon assembly, nested macros collapse into a single scope.
* But if a nested macro is called directly, the scopes above
* it will not be available.  So the language server flags
* this potentially erroneous reference in the definition.

main     MAC
loc1     iny
         bcc   loc2       ; this is OK
nest1    MAC
loc2     stx   1
         jmp   loc1       ; should be flagged
nest2    MAC
loc3     sty   2
         jmp   loc2       ; should be flagged
         <<<

         main
         nest1