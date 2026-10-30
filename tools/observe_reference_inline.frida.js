// Observe only the controlled fixture's character records and alignment inputs.
const base=Process.mainModule.base;
const address=value=>base.add(value-0x400000);
let records=0,alignments=0;
Interceptor.attach(address(0x115e5f0), {
  onEnter(args) { this.output=args[1]; this.line=args[2].toInt32(); this.capture=++records<=30; },
  onLeave() {
    if (!this.capture) return;
    try {
      const p=this.output.readPointer();
      if(p.isNull()) return;
      const n=p.sub(8).readU32();
      if(n>500) return;
      const chars=[];
      for(let i=0;i<n;i++) {
        const c=p.add(i*24);
        chars.push({c:String.fromCharCode(c.add(20).readU16()),type:c.readU8(),kind:c.add(1).readU8(),
          grammar:c.add(4).readS32(),segment:c.add(16).readS32(),continuation:c.add(12).readU8(),significant:c.add(13).readU8()});
      }
      send({kind:'chars',line:this.line,n,chars});
    }catch(error){send({kind:'read-error',error:String(error)});}
  }
});
Interceptor.attach(address(0x859830), {
  onEnter(args) {
    if(++alignments>30)return;
    const n=args[3].toInt32(),m=this.context.rsp.add(40).readS32();
    if(n<0||m<0||n>500||m>500)return;
    const read=(p,len)=>Array.from({length:len},(_,i)=>p.add(i*4).readU32());
    send({kind:'align32',n,m,left:read(args[1],n),right:read(args[2],m)});
  }
});
send({kind:'ready',pid:Process.id,record_limit:30,alignment_limit:30});
