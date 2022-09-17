10 d$ = chr$(4)

20 print d$;"open tree1,l128"
30 print d$;"write tree1,r2000"
40 print "HELLO FROM TREE 1"
50 print d$;"close tree1"

60 print d$;"open tree2,l127"
70 print d$;"write tree2,r2000"
80 print "HELLO FROM TREE 2"
90 print d$;"close tree2"

100 for i = 16384 to 32767: poke i,256*((i-16384)/256 - int((i-16384)/256)): next
110 print d$;"bsave sapling,a16384,l16384"
