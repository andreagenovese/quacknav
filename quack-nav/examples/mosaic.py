"""Tile many run PNGs into one sheet: python3 mosaic.py OUT.png run1.png run2.png ... (up to 64, 8 per row, each scaled by 1/2)."""
import sys, zlib, struct
def read_png(p):
    d=open(p,'rb').read(); assert d[:8]==b'\x89PNG\r\n\x1a\n'; i=8; idat=b''; w=h=0
    while i<len(d):
        n=struct.unpack('>I',d[i:i+4])[0]; t=d[i+4:i+8]; c=d[i+8:i+8+n]; i+=12+n
        if t==b'IHDR': w,h=struct.unpack('>II',c[:8])
        elif t==b'IDAT': idat+=c
    raw=zlib.decompress(idat); rows=[]; stride=w*3
    for y in range(h):
        f=raw[y*(stride+1)]; line=bytearray(raw[y*(stride+1)+1:(y+1)*(stride+1)])
        if f==0: pass
        else: raise SystemExit("solo filtro 0")
        rows.append(line)
    return w,h,rows
out=sys.argv[1]; files=sys.argv[2:64+2]
imgs=[read_png(f) for f in files]; w,h,_=imgs[0]; sw,sh=w//2,h//2; per=min(8,len(imgs)); nrow=(len(imgs)+per-1)//per
W,H=sw*per,sh*nrow; sheet=bytearray(b'\xff'*W*H*3)
for k,(w,h,rows) in enumerate(imgs):
    ox,oy=(k%per)*sw,(k//per)*sh
    for y in range(sh):
        src=rows[y*2]
        for x in range(sw):
            i=((oy+y)*W+ox+x)*3; sheet[i:i+3]=src[x*2*3:x*2*3+3]
def chunk(t,d): return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
data=b''.join(b'\x00'+bytes(sheet[y*W*3:(y+1)*W*3]) for y in range(H))
open(out,'wb').write(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',W,H,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(data,6))+chunk(b'IEND',b''))
print(out, f"{W}x{H}", len(imgs), "corse")
