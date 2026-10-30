// Read-only observation in the owned portable CLI replay. No target values are
// changed; the bounded log records only the module identity and section counts.
const exe = Process.mainModule;
send({kind:'target',path:exe.path,base:exe.base.toString(),pid:Process.id});
let count=0;
let labels=false;
Interceptor.attach(exe.base.add(0x859830-0x400000),{
  onEnter() {
    if(labels)return;
    labels=true;
    try {
      const table=exe.base.add(0x2130ab8-0x400000).readPointer();
      const values=[];
      for(let i=0;i<6;i++) {
        const value=table.add(i*8).readPointer();
        values.push(value.isNull()?null:value.readUtf16String(128).replace(/\0.*$/s,''));
      }
      send({kind:'result-labels',values});
    } catch(e) {send({kind:'read-error',message:String(e)});}
  }
});
Interceptor.attach(exe.base.add(0x118d2d0-0x400000),{
  onLeave(result) {if(count++<16)send({kind:'section-count',value:result.toInt32()});}
});
