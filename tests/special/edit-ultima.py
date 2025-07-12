import subprocess

# This makes a small edit to Ultima 5 STARTUP program menus.
# We can then try booting the disk in an emulator to see if the changes went through correctly.
# If it worked we should see "Mozying onward" as a menu option.

def verify(beg_vers: tuple, end_vers: tuple):
    '''Exit if a2kit version falls outside range beg_vers..end_vers'''
    vers = tuple(map(int, cmd(['-V']).decode('utf-8').split()[1].split('.')))
    if vers < beg_vers or vers >= end_vers:
        print("a2kit version outside range",beg_vers,"..",end_vers)
        exit(1)

def cmd(args, pipe_in=None):
    '''run a CLI command as a subprocess'''
    compl = subprocess.run(['a2kit']+args,input=pipe_in,capture_output=True,text=False)
    if compl.returncode>0:
        print(compl.stderr)
        exit(1)
    return compl.stdout

std_args = ['-d','write.woz','--pro','ultima5.fmt.json','-f','startup']
startup_prog = bytearray(cmd(['get'] + std_args + ['-t','bin']))
startup_prog[0x410:0x417] = bytes(map(lambda x: x+128,bytearray(b'Mozying')))
cmd(['delete'] + std_args)
cmd(['put'] + std_args + ['-t','bin','-a','32768'],startup_prog)

