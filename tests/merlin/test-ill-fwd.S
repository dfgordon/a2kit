beg      LDA   #$00
hello    MAC
         INX
         DFB   beg
         DFB   end
         JMP   cont
         DO    cont/2
         LUP   end/2
         INC
         --^
         FIN
cont     INX
         <<<
end      hello
         RTS