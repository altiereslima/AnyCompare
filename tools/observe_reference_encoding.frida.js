const cap = 200;
let sent = 0;
const handles = new Map();
const counts = new Map();
function emit(value) { if (sent++ < cap) send(value); }
function hook(name, callbacks) {
  try { const address = Process.getModuleByName("kernel32.dll").getExportByName(name); Interceptor.attach(address, callbacks); emit({kind:"hook",name,address:String(address)}); }
  catch (e) { emit({kind:"unavailable",name,error:String(e)}); }
}
hook("GetACP", {onLeave(result){emit({kind:"api",name:"GetACP",result:result.toUInt32()});}});
for (const name of ["MultiByteToWideChar","WideCharToMultiByte"]) {
 hook(name, {onEnter(args){this.cp=args[0].toUInt32();this.flags=args[1].toUInt32();this.length=args[3].toInt32();},onLeave(result){const key=name+":"+this.cp+":"+this.flags;counts.set(key,(counts.get(key)||0)+1);if(counts.get(key)===1)emit({kind:"api-first",name,code_page:this.cp,flags:this.flags,input_length:this.length,result_length:result.toInt32()});}});
}
hook("CreateFileW",{onEnter(args){ this.owned=null;try { const p=args[0].readUtf16String();if (p && (/\\anycompare\\docs\\reference\\controlled-(full-)?replay\\/i.test(p) || /\\BC(State|Sessions|Preferences)\.xml$/i.test(p))) this.owned=p;}catch(_){} },onLeave(result){if(this.owned){handles.set(String(result),this.owned);emit({kind:"file-open",path:this.owned,handle:String(result)});}}});
for (const name of ["ReadFile","WriteFile"]) {
 hook(name,{onEnter(args){this.owned=handles.get(String(args[0]));this.requested=args[2].toUInt32();},onLeave(result){if(this.owned){const key=name+":"+this.owned;counts.set(key,(counts.get(key)||0)+1);if(counts.get(key)===1)emit({kind:"io-first",name,path:this.owned,requested_bytes:this.requested,success:result.toInt32()!==0});}}});
}
hook("ExitProcess",{onEnter(args){emit({kind:"api-counts",counts:Object.fromEntries(counts)});emit({kind:"exit",code:args[0].toUInt32()});}});
emit({kind:"target",pid:Process.id,architecture:Process.arch,main_module:Process.mainModule.path});
