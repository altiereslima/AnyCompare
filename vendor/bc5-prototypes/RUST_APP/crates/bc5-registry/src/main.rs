use bc5_registry::{compare, read_file, read_live_local};
use std::path::Path;
fn load(arg:&str)->Result<bc5_registry::Registry,String>{
    if let Some(key)=arg.strip_prefix("reg:\\\\") {read_live_local(key)}
    else {read_file(Path::new(arg))}
}
fn main(){
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=3 {eprintln!("Uso: bc5-registry-inspect <esquerda.reg|reg:\\\\HKEY_...> <direita.reg|reg:\\\\HKEY_...>");std::process::exit(2);}
    match (load(&args[1]),load(&args[2])) {
        (Ok(a),Ok(b))=>{let r=compare(&a,&b);print!("{}",r.text_report());if !r.difference_indices().is_empty(){std::process::exit(1);}},
        (Err(e),_)|(_,Err(e))=>{eprintln!("Erro: {e}");std::process::exit(2);}
    }
}
