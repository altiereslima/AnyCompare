// Read-only WM_GETFONT + LOGFONT inspection of the explicitly selected BC4
// process. No text, file paths, preferences, handles or window contents emitted.
const u=Process.getModuleByName('user32.dll'), g=Process.getModuleByName('gdi32.dll');
const enumerate=new NativeFunction(u.getExportByName('EnumChildWindows'),'int',['pointer','pointer','pointer']);
const enumerateTop=new NativeFunction(u.getExportByName('EnumWindows'),'int',['pointer','pointer']);
const owner=new NativeFunction(u.getExportByName('GetWindowThreadProcessId'),'uint',['pointer','pointer']);
const className=new NativeFunction(u.getExportByName('GetClassNameW'),'int',['pointer','pointer','int']);
const message=new NativeFunction(u.getExportByName('SendMessageTimeoutW'),'pointer',['pointer','uint','pointer','pointer','uint','uint','pointer']);
const getObject=new NativeFunction(g.getExportByName('GetObjectW'),'int',['pointer','int','pointer']);
const seen=new Set(); let examined=0;
function inspect(hwnd) {
  if(++examined>1500)return;
  const result=Memory.alloc(Process.pointerSize);
  result.writePointer(ptr(0));
  // WM_GETFONT, abort if a window is unresponsive; never write to its control.
  if(message(hwnd,0x31,ptr(0),ptr(0),2,100,result).isNull())return;
  const font=result.readPointer(); if(font.isNull())return;
  const lf=Memory.alloc(92); if(getObject(font,92,lf)!==92)return;
  const cls=Memory.alloc(256); const n=className(hwnd,cls,128);
  const data={class:n>0?cls.readUtf16String(n):'',height:lf.readS32(),weight:lf.add(16).readS32(),quality:lf.add(26).readU8(),face:lf.add(28).readUtf16String(32).replace(/\0.*$/s,'')};
  const key=JSON.stringify(data);if(!seen.has(key)){seen.add(key);send(data);}
}
const child=new NativeCallback((hwnd)=>{inspect(hwnd);return 1;},'int',['pointer','pointer']);
const top=new NativeCallback((hwnd)=>{
  const pid=Memory.alloc(4);owner(hwnd,pid);
  if(pid.readU32()===Process.id){inspect(hwnd);enumerate(hwnd,child,ptr(0));}
  return 1;
},'int',['pointer','pointer']);
enumerateTop(top,ptr(0));
send({kind:'complete',pid:Process.id,examined,unique:seen.size});
