* This is the part of DOS 3.2 that writes the data prolog
* upon any write.  Also serves as a test of Merlin parsing.
_B87D    LDA   #$FF       ; sync-byte value (first 8 bits)
         STA   $C08F,X    ; write mode
         ORA   $C08C,X    ; start write
         PHA              ; timing
         PLA              ; timing
         NOP              ; timing
         LDY   #$0A       ; 10 sync bytes
_B88A    ORA   $26        ; timing
         JSR   $B8F4      ; write nibble
         DEY
         BNE   $B88A
         LDA   #$D5
         JSR   $B8F3
         LDA   #$AA
         JSR   $B8F3
         LDA   #$AD
         JSR   $B8F3


