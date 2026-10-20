import io, struct, os
from PIL import Image
R={k:Image.open(f'R_{k}.png').convert('RGBA') for k in ('mac_full','mac_simple','full_full','full_simple')}
def sz(k,n): return R[k].resize((n,n),Image.LANCZOS)
def mac(n): return sz('mac_simple' if n<=64 else 'mac_full', n)
def full(n): return sz('full_simple' if n<=50 else 'full_full', n)
O='out'; P=lambda p: (os.makedirs(os.path.dirname(f'{O}/{p}'),exist_ok=True), f'{O}/{p}')[1]
def png(im): b=io.BytesIO(); im.save(b,'PNG',optimize=True); return b.getvalue()
types=[('icp4',16),('icp5',32),('icp6',64),('ic07',128),('ic08',256),('ic09',512),('ic10',1024),('ic11',32),('ic12',64),('ic13',256),('ic14',512)]
body=b''.join(t.encode()+struct.pack('>I',8+len(d))+d for t,n in types for d in [png(mac(n))])
open(P('src-tauri/icons/icon.icns'),'wb').write(b'icns'+struct.pack('>I',8+len(body))+body)
sizes=[16,24,32,48,64,128,256]; ims=[png(full(n)) for n in sizes]; off=6+16*len(sizes); ent=b''
for n,d in zip(sizes,ims): ent+=struct.pack('<BBBBHHII',n%256,n%256,0,0,1,32,len(d),off); off+=len(d)
open(P('src-tauri/icons/icon.ico'),'wb').write(struct.pack('<HHH',0,1,len(sizes))+ent+b''.join(ims))
for name,n in [('icon.png',512),('32x32.png',32),('64x64.png',64),('128x128.png',128),('128x128@2x.png',256),('StoreLogo.png',50)]+[(f'Square{n}x{n}Logo.png',n) for n in (30,44,71,89,107,142,150,284,310)]:
    full(n).save(P('src-tauri/icons/'+name))
mac(1024).save(P('assets/formdesign_icon_macos.png')); full(1024).save(P('assets/formdesign_icon.png')); full(224).save(P('assets/formdesign_folder_icon.png'))
at=Image.new('RGBA',(180,180),(16,15,20,255)); at.alpha_composite(full(180)); at.convert('RGB').save(P('frontend/public/apple-touch-icon.png'))
fs=[16,32,48]; fi=[png(full(n)) for n in fs]; off=6+16*3; ent=b''
for n,d in zip(fs,fi): ent+=struct.pack('<BBBBHHII',n,n,0,0,1,32,len(d),off); off+=len(d)
open(P('frontend/public/favicon.ico'),'wb').write(struct.pack('<HHH',0,1,3)+ent+b''.join(fi))
