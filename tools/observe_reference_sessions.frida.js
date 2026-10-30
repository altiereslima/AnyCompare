// Read-only audit of SessionsCombo in the explicitly selected isolated BC4 process.
// No hooks, input injection, target calls, document contents or profile writes.
const m=Process.mainModule;
if(!/\\reference-sessions-runtime\\BC4Reference\.exe$/i.test(m.path)) throw new Error('Unexpected reference process');
const originalBase=ptr('0x400000');
const address=va=>m.base.add(ptr(va).sub(originalBase));
const vmt=address('0xeb4dc0'), select=address('0x1905fa0');
let examined=0, skipped=0;const controls=[];
for(const range of Process.enumerateRanges({protection:'rw-',coalesce:true})){
 if(range.size>64*1024*1024 || examined+range.size>192*1024*1024){skipped+=range.size;continue;}
 examined+=range.size;
 for(const match of Memory.scanSync(range.base,range.size,vmt.toMatchPattern())){
  try{
   const object=match.address;
   if(!object.add(0x500).readPointer().equals(select))continue;
   const tree=object.add(0x488).readPointer();
   controls.push({object:object.toString(),dropDownCount:object.add(0x4d0).readS32(),
     button:{width:object.add(0x98).readS32(),height:object.add(0x9c).readS32()},
     tree:{width:tree.add(0x98).readS32(),height:tree.add(0x9c).readS32()}});
  }catch(error){/* Non-object pointer copies are not accepted as controls. */}
 }
}
send({kind:'sessions-control',pid:Process.id,path:m.path,base:m.base.toString(),examined_bytes:examined,skipped_bytes:skipped,controls});

const u=Process.getModuleByName('user32.dll'),g=Process.getModuleByName('gdi32.dll');
const enumTop=new NativeFunction(u.getExportByName('EnumWindows'),'int',['pointer','pointer']);
const getPid=new NativeFunction(u.getExportByName('GetWindowThreadProcessId'),'uint',['pointer','pointer']);
const className=new NativeFunction(u.getExportByName('GetClassNameW'),'int',['pointer','pointer','int']);
const isVisible=new NativeFunction(u.getExportByName('IsWindowVisible'),'int',['pointer']);
const rect=new NativeFunction(u.getExportByName('GetWindowRect'),'int',['pointer','pointer']);
const dc=new NativeFunction(u.getExportByName('GetDC'),'pointer',['pointer']);
const release=new NativeFunction(u.getExportByName('ReleaseDC'),'int',['pointer','pointer']);
const pixel=new NativeFunction(g.getExportByName('GetPixel'),'uint',['pointer','int','int']);
const enumChildren=new NativeFunction(u.getExportByName('EnumChildWindows'),'int',['pointer','pointer','pointer']);
function inspectWindow(hwnd){
 const pid=Memory.alloc(4);getPid(hwnd,pid);if(pid.readU32()!==Process.id||!isVisible(hwnd))return 1;
 const name=Memory.alloc(256);const n=className(hwnd,name,128), cls=n?name.readUtf16String(n):'';
 if(!/tree/i.test(cls))return 1;
 const bounds=Memory.alloc(16);rect(hwnd,bounds);
 const context=dc(hwnd), samples=[];
 if(!context.isNull()){
  for(let y=0;y<18;y++){const c=pixel(context,40,y);samples.push([c&255,(c>>>8)&255,(c>>>16)&255]);}
  release(hwnd,context);
 }
 send({kind:'visible-tree',class:cls,width:bounds.add(8).readS32()-bounds.readS32(),height:bounds.add(12).readS32()-bounds.add(4).readS32(),selection_gap_column:samples});
 return 1;
}
const child=new NativeCallback(hwnd=>inspectWindow(hwnd),'int',['pointer','pointer']);
const top=new NativeCallback(hwnd=>{ const pid=Memory.alloc(4);getPid(hwnd,pid);if(pid.readU32()===Process.id){inspectWindow(hwnd);enumChildren(hwnd,child,ptr(0));}return 1; },'int',['pointer','pointer']);
enumTop(top,ptr(0));
