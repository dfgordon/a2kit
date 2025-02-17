* Test whether dependencies are resolved and whether
* conditionals are resolved or deferred.
* The expansion is tested manually, conditionals will
* be resolved if the nested macro call is commented out.

]dolda   =     2

m1       MAC
]dolda   =     0
         inx
         <<<

m2       MAC
         DO    ]dolda
         LDA   0
         FIN
         m1
         <<<

doldx    =     0

m3       MAC
         MX    5
         IF    MX/2
         PLA
         FIN
         m2
         IF    MX/2
         PLA
         FIN
         if    #=]1
         INY
         fin
         DO    ]dolda-1
         LDA   0
         FIN
         DO    doldx/2
         LDX   0
         FIN
         DO    doldy
         LDY   0
         FIN
         <<<

]dolda   =     1
doldy    =     2

unused   MAC
         LDA   undefined  ; should not be flagged
         <<<

         mx    0
         m3    #1
