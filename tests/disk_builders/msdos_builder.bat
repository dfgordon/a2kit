mkdir dir1
copy a:\dskbld.bas a:\dir1\dskbld.bas
cd dir1
c:\util\basic dskbld.bas
mkdir subdir1
copy a:\dskbld.bas a:\dir1\subdir1\dskbld.bas
cd subdir1
c:\util\basic dskbld.bas
rename ascend.txt up.txt
cd ..
cd ..
mkdir dir2
mkdir dir3
copy a:\dir1\ascend.txt a:\dir2\ascend.txt
copy a:\dir1\ascend.txt a:\dir3\ascend.txt
del a:\dir2\ascend.txt
rmdir a:\dir2
