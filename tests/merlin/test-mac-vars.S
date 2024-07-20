* Merlin 16 or higher should reject the `]0` variables.
* The `]9` should always be OK.
]1       inx
]5       jmp   0
]0       =     5
]9       =     $FC10
         asc   ]1
mymac    mac
         asc   ]1
         inc   ]2
         eom
         jmp   ]5
         jmp   ]0
         jsr   ]9
         if    a=]1
         iny
         fin
