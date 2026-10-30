// Read-only hooks in the owned portable reference replay. No return values or
// target buffers are changed. Bound all observations; emit no user profile data.
const exe = Process.mainModule;
let count = 0;
const seenFonts = new Set();
function emit(value) { if (count++ < 100) send(value); }
const gdi = Process.getModuleByName('gdi32.dll');
Interceptor.attach(gdi.getExportByName('CreateFontIndirectW'), {
  onEnter(args) {
    try {
      const p = args[0];
      const font = {height:p.readS32(), width:p.add(4).readS32(), weight:p.add(16).readS32(),
        quality:p.add(26).readU8(), face:p.add(28).readUtf16String(32).replace(/\0.*$/s,'')};
      const key = JSON.stringify(font);
      if (!seenFonts.has(key)) { seenFonts.add(key); emit({kind:'font', ...font}); }
    } catch (error) { emit({kind:'font-error', message:String(error)}); }
  }
});
for (const [address, unit] of [[0x857bb0,1],[0x859830,4]]) {
  let calls = 0;
  Interceptor.attach(exe.base.add(address - 0x400000), {
    onEnter(args) {
      if (++calls > 8) return;
      const n = args[3].toInt32();
      const m = this.context.rsp.add(40).readS32();
      const stack = Thread.backtrace(this.context,Backtracer.ACCURATE).slice(0,12)
        .map(p => p.sub(exe.base).add(0x400000).toString());
      let left=null,right=null;
      if (unit===1 && n>=0 && m>=0 && n<=512 && m<=512) {
        left=Array.from(new Uint8Array(args[1].readByteArray(n)));
        right=Array.from(new Uint8Array(args[2].readByteArray(m)));
      }
      emit({kind:'align',address:'0x'+address.toString(16),unit,n,m,left,right,stack});
    }
  });
}
emit({kind:'ready', module:exe.name, base:exe.base.toString(), pid:Process.id, limit:100});
