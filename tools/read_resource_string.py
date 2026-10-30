import struct,sys,os
sys.stdout.reconfigure(encoding='utf-8')
def strings(path, ident):
    d=open(path,'rb').read()
    pe=struct.unpack_from('<I',d,0x3c)[0]; nsec=struct.unpack_from('<H',d,pe+6)[0]; optsz=struct.unpack_from('<H',d,pe+20)[0]; opt=pe+24
    magic=struct.unpack_from('<H',d,opt)[0]; dd=opt+(112 if magic==0x20b else 96)
    rva,size=struct.unpack_from('<II',d,dd+2*8)
    secs=[]
    for i in range(nsec):
        s=opt+optsz+40*i; vs,va,rs,raw=struct.unpack_from('<IIII',d,s+8); secs.append((va,max(vs,rs),raw))
    def off(r):
        for va,sz,raw in secs:
            if va<=r<va+sz: return raw+r-va
    base=off(rva)
    def entries(o):
        n=struct.unpack_from('<HH',d,o+12); out=[]
        for k in range(n[0]+n[1]):
            name,ptr=struct.unpack_from('<II',d,o+16+8*k); out.append((name,ptr))
        return out
    block=ident//16+1; idx=ident%16
    for name,ptr in entries(base):
        if name==6:
            for n2,p2 in entries(base+(ptr&0x7fffffff)):
                if n2==block:
                    for n3,p3 in entries(base+(p2&0x7fffffff)):
                        de=base+(p3&0x7fffffff); drva,dsz=struct.unpack_from('<II',d,de); o=off(drva)
                        for j in range(16):
                            ln=struct.unpack_from('<H',d,o)[0]; s=d[o+2:o+2+ln*2].decode('utf-16-le'); o+=2+ln*2
                            if j==idx: return s
    return None
for path in sys.argv[2:]:
    try: print(os.path.basename(path), repr(strings(path,int(sys.argv[1]))))
    except Exception as ex: print(os.path.basename(path), 'erro', ex)
