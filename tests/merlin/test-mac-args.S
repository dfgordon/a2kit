mymac    MAC
         lda   ]1
         sta   ]2
         stx   ]3
         EOM

* test uninterpreted arg
         mymac #$c5;$00;!hello

* test too few args
         mymac 1;2

* test too many args
         mymac 1;2;4;5

condmac  MAC
         DO    1
         lda   ]1
         ELSE
         lda   ]1
         lda   ]2
         FIN
         <<<

* test conditionally too few args
         condmac 1

* test conditionally too many args
         condmac 1;2

* test unconditionally too many args
         condmac 1;2;3