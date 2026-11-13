//! Núcleo de análise de Registro Windows. Sem escrita no Registro.
//! A semântica abaixo é um protótipo verificável, NÃO um clone 1:1 validado.
use std::{collections::{BTreeMap, BTreeSet}, fs, path::Path, io::Read};

mod export;
mod edit;
mod native_buffers;
mod inline;
pub use inline::{character_difference, CharacterDifference};
pub use export::{export_reg, export_subtree};
pub use edit::SnapshotEditor;

pub const MAX_REG_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_REG_LINES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegValue {
    pub name: String, // nome vazio representa valor padrão (@)
    pub type_id: u32,
    pub bytes: Vec<u8>, // representação binária e tipo, nunca apenas texto formatado
}
impl RegValue {
    pub fn type_name(&self) -> String {
        match self.type_id {
            0 => "REG_NONE".into(), 1 => "REG_SZ".into(),
            2 => "REG_EXPAND_SZ".into(), 3 => "REG_BINARY".into(),
            4 => "REG_DWORD".into(), 5 => "REG_DWORD_BIG_ENDIAN".into(),
            6 => "REG_LINK".into(), 7 => "REG_MULTI_SZ".into(),
            8 => "REG_RESOURCE_LIST".into(), 9 => "REG_FULL_RESOURCE_DESCRIPTOR".into(),
            10 => "REG_RESOURCE_REQUIREMENTS_LIST".into(), 11 => "REG_QWORD".into(),
            n => format!("REG_TYPE_0x{n:X}"),
        }
    }
    /// Prévia limitada para tabelas e relatórios: nunca formata um blob binário
    /// de vários MiB a cada quadro da interface. Não altera os bytes comparados.
    pub fn displayed_preview(&self, max_chars: usize) -> String {
        let max_chars = max_chars.clamp(16, 512);
        if self.bytes.len() > 4096 {
            let max_hex_bytes = (max_chars.saturating_sub(12) / 3).clamp(1,64);
            let preview = hex(&self.bytes[..self.bytes.len().min(max_hex_bytes)]);
            return format!("{preview} … ({} bytes; prévia limitada)",self.bytes.len());
        }
        let text = self.displayed();
        let mut out: String = text.chars().take(max_chars + 1).collect();
        if out.chars().count() > max_chars {
            out = out.chars().take(max_chars).collect();
            out.push_str("… (prévia limitada)");
        }
        out
    }
    pub fn displayed(&self) -> String {
        match (self.type_id, self.bytes.as_slice()) {
            (1 | 2 | 6, bytes) => utf16_text(bytes).unwrap_or_else(|| hex(bytes)),
            (7, bytes) => utf16_multi(bytes).unwrap_or_else(|| hex(bytes)),
            (4, bytes) if bytes.len() == 4 => format!("0x{:08X} ({})", u32::from_le_bytes(bytes.try_into().unwrap()), u32::from_le_bytes(bytes.try_into().unwrap())),
            (5, bytes) if bytes.len() == 4 => format!("0x{:08X}", u32::from_be_bytes(bytes.try_into().unwrap())),
            (11, bytes) if bytes.len() == 8 => format!("0x{:016X} ({})", u64::from_le_bytes(bytes.try_into().unwrap()), u64::from_le_bytes(bytes.try_into().unwrap())),
            (_, bytes) => hex(bytes),
        }
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
fn utf16_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 { return None; }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0],c[1]])).collect();
    String::from_utf16(units.split(|c| *c == 0).next().unwrap_or(&[])).ok()
}
fn utf16_multi(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 { return None; }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0],c[1]])).collect();
    let mut parts=Vec::new();
    for s in units.split(|c| *c == 0) {
        if s.is_empty() { break; }
        parts.push(String::from_utf16(s).ok()?);
    }
    Some(parts.join(" | "))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegKey { pub path: String, pub values: BTreeMap<String, RegValue> }
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry { pub keys: BTreeMap<String, RegKey> }
fn canonical(v: &str) -> String { v.to_lowercase() }
fn norm_key(v: &str) -> String { canonical(v.trim_matches('\\')) }

fn ensure_parent_keys(reg: &mut Registry, full: &str) {
    let mut part=String::new();
    for component in full.split('\\') {
        if !part.is_empty() { part.push('\\'); }
        part.push_str(component);
        let key=norm_key(&part);
        reg.keys.entry(key).or_insert_with(|| RegKey{path:part.clone(),values:BTreeMap::new()});
    }
}
fn unescape_quoted(source: &str, offset: usize) -> Result<(String, usize), String> {
    if !source[offset..].starts_with('"') { return Err("campo sem aspas".into()); }
    let mut chars=source[offset+1..].char_indices();
    let mut out=String::new();
    while let Some((i,c))=chars.next() {
        match c {
            '"' => return Ok((out,offset+1+i+1)),
            '\\' => match chars.next() {
                Some((_, '"'))=>out.push('"'),
                Some((_, '\\'))=>out.push('\\'),
                Some((_, other))=>return Err(format!("escape não suportado: \\{other}")),
                None=>return Err("barra final incompleta".into()),
            },
            c => out.push(c),
        }
    }
    Err("aspas não fechadas".into())
}
fn parse_bytes(csv: &str) -> Result<Vec<u8>, String> {
    if csv.trim().is_empty() { return Ok(Vec::new()); }
    csv.split(',').map(|x| {
        let token=x.trim();
        if token.len()!=2 || !token.is_ascii() { return Err(format!("byte hexadecimal inválido: {token}")); }
        u8::from_str_radix(token,16).map_err(|_|format!("byte hexadecimal inválido: {token}"))
    }).collect()
}
fn parse_rhs(name: String, rhs: &str) -> Result<RegValue, String> {
    let rhs=rhs.trim();
    let (type_id,bytes)=if rhs.starts_with('"') {
        let (s,end)=unescape_quoted(rhs,0)?;
        if !rhs[end..].trim().is_empty() { return Err("sufixo após texto".into()); }
        let mut bytes=Vec::with_capacity(s.len()*2+2);
        for code in s.encode_utf16().chain(std::iter::once(0)) { bytes.extend_from_slice(&code.to_le_bytes()); }
        (1,bytes)
    } else if rhs.get(..6).is_some_and(|p|p.eq_ignore_ascii_case("dword:")) {
        let number=&rhs[6..];
        if number.len()!=8 || !number.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("DWORD exige 8 dígitos hex".into()); }
        (4,u32::from_str_radix(number,16).map_err(|e|e.to_string())?.to_le_bytes().to_vec())
    } else if rhs.get(..4).is_some_and(|p|p.eq_ignore_ascii_case("hex:")) {
        (3,parse_bytes(&rhs[4..])?)
    } else if rhs.get(..4).is_some_and(|p|p.eq_ignore_ascii_case("hex(")) {
        let close=rhs.find("):").ok_or("hex(t) sem delimitador")?;
        let n=u32::from_str_radix(&rhs[4..close],16).map_err(|_|"tipo hexadecimal inválido")?;
        let after=&rhs[close+2..];
        (n,parse_bytes(after)?)
    } else { return Err(format!("valor de Registro não reconhecido: {rhs}")); };
    Ok(RegValue{name,type_id,bytes})
}

fn decode_reg(bytes: &[u8]) -> Result<String,String> {
    if bytes.len() > MAX_REG_BYTES { return Err("arquivo .reg acima do limite de 64 MiB".into()); }
    if bytes.starts_with(&[0xff,0xfe]) {
        if (bytes.len()-2)%2!=0 { return Err("UTF-16LE incompleto".into()); }
        let words:Vec<_>=bytes[2..].chunks_exact(2).map(|x|u16::from_le_bytes([x[0],x[1]])).collect();
        return String::from_utf16(&words).map_err(|_|"UTF-16LE inválido".into());
    }
    if bytes.starts_with(&[0xfe,0xff]) {
        if (bytes.len()-2)%2!=0 { return Err("UTF-16BE incompleto".into()); }
        let words:Vec<_>=bytes[2..].chunks_exact(2).map(|x|u16::from_be_bytes([x[0],x[1]])).collect();
        return String::from_utf16(&words).map_err(|_|"UTF-16BE inválido".into());
    }
    let b=bytes.strip_prefix(&[0xef,0xbb,0xbf]).unwrap_or(bytes);
    // REGEDIT4 sem BOM utiliza a página de códigos ANSI da máquina Windows.
    // Não interpretar esses bytes como UTF-8 quando a página local é diferente.
    #[cfg(windows)]
    if b.starts_with(b"REGEDIT4") && !bytes.starts_with(&[0xef,0xbb,0xbf]) {
        return decode_regedit4_acp(b);
    }
    String::from_utf8(b.to_vec()).map_err(|_|"REGEDIT4 ANSI requer a página de códigos do Windows; bytes desconhecidos".into())
}

/// Conversão ANSI específica para REGEDIT4, usando a API nativa da máquina
/// Windows onde o BC5 está executando. Sem adivinhar CP1252 no Linux.
#[cfg(windows)]
fn decode_regedit4_acp(bytes: &[u8]) -> Result<String, String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn MultiByteToWideChar(codepage: u32, flags: u32, src: *const u8,
            src_len: i32, dest: *mut u16, dest_len: i32) -> i32;
    }
    let len = i32::try_from(bytes.len()).map_err(|_|"REGEDIT4 grande demais")?;
    if len == 0 {return Err("REGEDIT4 vazio".into());}
    // CP_ACP=0: codificação ANSI configurada pelo Windows, não pela aplicação.
    let need = unsafe {MultiByteToWideChar(0,0,bytes.as_ptr(),len,std::ptr::null_mut(),0)};
    if need <= 0 {return Err("falha ao decodificar REGEDIT4 com CP_ACP".into());}
    let mut words = vec![0u16;need as usize];
    let got = unsafe {MultiByteToWideChar(0,0,bytes.as_ptr(),len,words.as_mut_ptr(),need)};
    if got != need {return Err("conversão ANSI incompleta".into());}
    String::from_utf16(&words).map_err(|_|"conversão ANSI gerou UTF-16 inválido".into())
}

pub fn parse_reg(bytes: &[u8]) -> Result<Registry,String> {
    let text=decode_reg(bytes)?;
    let mut reg=Registry::default();
    let mut header=false;
    let mut current:Option<String>=None;
    let mut combined=String::new();
    let mut pending=false;
    for (idx,line) in text.lines().enumerate() {
        if idx>=MAX_REG_LINES {return Err("número de linhas acima do limite".into());}
        let line=line.trim_end_matches('\r').trim();
        if pending {
            combined.push_str(line);
        } else {
            combined.clear();combined.push_str(line);
        }
        // Continuação apenas nas listas hexadecimais (evita tratar strings escapadas como continuação).
        if (pending || combined.to_ascii_lowercase().contains("=hex:") || combined.to_ascii_lowercase().contains("=hex(")) && combined.ends_with('\\') {
            combined.pop();pending=true;continue;
        }
        pending=false;
        let line=combined.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {continue;}
        if !header {
            if line != "Windows Registry Editor Version 5.00" && line != "REGEDIT4" {
                return Err(format!("linha {}: cabeçalho de .reg não reconhecido",idx+1));
            }
            header=true;continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name=&line[1..line.len()-1];
            if name.is_empty() || name.starts_with('\\') || name.ends_with('\\') || name.split('\\').any(str::is_empty) || name.contains('\0') {
                return Err(format!("linha {}: caminho de chave inválido",idx+1));
            }
            if name.starts_with('-') {return Err(format!("linha {}: exclusão de chave é patch, não snapshot",idx+1));}
            let root=name.split('\\').next().unwrap_or("").to_ascii_uppercase();
            if !["HKEY_CLASSES_ROOT","HKEY_CURRENT_USER","HKEY_LOCAL_MACHINE","HKEY_USERS","HKEY_CURRENT_CONFIG"].contains(&root.as_str()) {
                return Err(format!("linha {}: raiz de Registro inválida",idx+1));
            }
            ensure_parent_keys(&mut reg,name);
            current=Some(norm_key(name));continue;
        }
        let key=current.as_ref().ok_or_else(||format!("linha {}: valor fora da chave",idx+1))?;
        let (name,rest)=if let Some(rhs)=line.strip_prefix('@') {
            (String::new(),rhs)
        } else if line.starts_with('"') {
            let (n,end)=unescape_quoted(line,0)?;
            (n,&line[end..])
        } else {return Err(format!("linha {}: nome de valor inválido",idx+1));};
        let rhs=rest.trim_start().strip_prefix('=').ok_or_else(||format!("linha {}: '=' ausente",idx+1))?;
        if rhs.trim()=="-" {return Err(format!("linha {}: exclusão de valor é patch, não snapshot",idx+1));}
        let v=parse_rhs(name.clone(),rhs).map_err(|e|format!("linha {}: {e}",idx+1))?;
        let dest=reg.keys.get_mut(key).ok_or("chave desapareceu")?;
        let id=canonical(&name);
        match dest.values.get(&id) {
            Some(existing) if existing.type_id != v.type_id || existing.bytes != v.bytes => return Err(format!("linha {}: valor repetido conflitante",idx+1)),
            Some(_) => {},
            None => {dest.values.insert(id,v);}
        }
    }
    if pending {return Err("última linha hex terminada por continuação incompleta".into());}
    if !header {return Err("arquivo sem cabeçalho .reg".into());}
    Ok(reg)
}

pub fn read_file(path: &Path) -> Result<Registry,String> {
    let meta=fs::metadata(path).map_err(|e|e.to_string())?;
    if meta.len()>MAX_REG_BYTES as u64 {return Err("arquivo .reg excede 64 MiB".into());}
    let f=fs::File::open(path).map_err(|e|e.to_string())?;
    let mut bytes=Vec::new();
    f.take(MAX_REG_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()>MAX_REG_BYTES {return Err("arquivo .reg cresceu acima de 64 MiB".into());}
    parse_reg(&bytes)
}

/// Vista nativa ou vista WOW64 explícita para leitura, sem modificar o Registro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryView { Native, View32, View64 }

#[cfg(windows)]
mod native_windows;

/// Consulta somente leitura via APIs nativas. Para host remoto, requer o serviço
/// Remote Registry, permissões da conta e suporte Windows; erros não são ignorados.
pub fn read_live_with_options(host: Option<&str>, key: &str, view: RegistryView)
    -> Result<Registry, String> {
    #[cfg(windows)]
    { native_windows::read_native(host, key, view) }
    #[cfg(not(windows))]
    { let _=(host,key,view); Err("Registro vivo disponível apenas no Windows".into()) }
}
pub fn read_live_local(key: &str) -> Result<Registry, String> {
    read_live_with_options(None, key, RegistryView::Native)
}
pub fn read_live_remote(machine: &str, key: &str, view: RegistryView)
    -> Result<Registry, String> {
    read_live_with_options(Some(machine), key, view)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difference { Same, Changed, LeftOnly, RightOnly }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind { Key, Value }
#[derive(Clone, Debug)]
pub struct DiffRow {
    pub path:String,
    /// Chave de origem separada do nome do valor; nomes podem conter barras inversas.
    pub key_path:String,
    pub name:String,
    pub level:usize,
    pub item:ItemKind,
    pub status:Difference,
    pub left:Option<RegValue>,
    pub right:Option<RegValue>,
}
#[derive(Clone, Debug, Default)]
pub struct CompareResult {pub rows:Vec<DiffRow>}
impl CompareResult {
    pub fn changed_count(&self)->usize{ self.rows.iter().filter(|r|r.item==ItemKind::Value && r.status==Difference::Changed).count() }
    pub fn orphan_count(&self)->usize{ self.rows.iter().filter(|r|r.status==Difference::LeftOnly || r.status==Difference::RightOnly).count() }
    pub fn difference_indices(&self)->Vec<usize>{self.rows.iter().enumerate().filter_map(|(i,x)| (x.status!=Difference::Same).then_some(i)).collect()}
    pub fn text_report(&self)->String{
        let mut out=String::from("Registry Compare — relatório experimental, não validado 1:1 contra BC5\n");
        const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
        for r in &self.rows {
            if r.status==Difference::Same {continue;}
            let ty=if r.item==ItemKind::Key {"Chave"}else{"Valor"};
            let line=format!("{ty}\t{:?}\t{}\t{}\t{}\n",r.status,r.path,
                r.left.as_ref().map(|v|format!("{}: {}",v.type_name(),v.displayed_preview(256))).unwrap_or_default(),
                r.right.as_ref().map(|v|format!("{}: {}",v.type_name(),v.displayed_preview(256))).unwrap_or_default());
            if out.len().saturating_add(line.len()) > MAX_REPORT_BYTES {
                out.push_str("[RELATÓRIO TRUNCADO: limite de segurança de 8 MiB; não é relatório BC5]\n");
                break;
            }
            out.push_str(&line);
        }
        out
    }
}
pub fn compare(left:&Registry,right:&Registry)->CompareResult{
    let mut result=CompareResult::default();
    let keys:BTreeSet<&String>=left.keys.keys().chain(right.keys.keys()).collect();
    for key in keys {
        let l=left.keys.get(key);
        let r=right.keys.get(key);
        let values:BTreeSet<&String>=l.into_iter().flat_map(|x|x.values.keys()).chain(r.into_iter().flat_map(|x|x.values.keys())).collect();
        let status=if l.is_none(){Difference::RightOnly}else if r.is_none(){Difference::LeftOnly}else if values.iter().any(|name| {
            match (l.and_then(|x|x.values.get(*name)),r.and_then(|x|x.values.get(*name))) {
                (Some(a),Some(b)) => a.type_id!=b.type_id || a.bytes!=b.bytes,
                (None,None) => false,
                _ => true,
            }
        }){Difference::Changed}else{Difference::Same};
        let display=l.or(r).unwrap().path.clone();
        result.rows.push(DiffRow{path:display.clone(),key_path:display.clone(),name:display.rsplit('\\').next().unwrap_or(&display).into(),level:display.split('\\').count().saturating_sub(1),item:ItemKind::Key,status,left:None,right:None});
        for value in values {
            let a=l.and_then(|x|x.values.get(value)).cloned();
            let b=r.and_then(|x|x.values.get(value)).cloned();
            let stat=match (&a,&b){(Some(a),Some(b))=>if a.type_id==b.type_id && a.bytes==b.bytes {Difference::Same}else{Difference::Changed},(Some(_),None)=>Difference::LeftOnly,(None,Some(_))=>Difference::RightOnly,_=>unreachable!()};
            let name=a.as_ref().or(b.as_ref()).unwrap().name.clone();
            result.rows.push(DiffRow{path:format!("{}\\{}",display,if name.is_empty(){"(Padrão)"}else{&name}),key_path:display.clone(),name,level:display.split('\\').count(),item:ItemKind::Value,status:stat,left:a,right:b});
        }
    }
    // Propagar as diferenças aos ancestrais sem busca quadrática em todos os registros.
    let key_positions:BTreeMap<String,usize>=result.rows.iter().enumerate()
        .filter(|(_,r)|r.item==ItemKind::Key).map(|(i,r)|(canonical(&r.path),i)).collect();
    for i in (0..result.rows.len()).rev() {
        if result.rows[i].status==Difference::Same {continue;}
        let path=result.rows[i].key_path.clone();
        let mut parent=path.as_str();
        while let Some((p,_))=parent.rsplit_once('\\') {
            if let Some(&index)=key_positions.get(&canonical(p)) {
                if result.rows[index].status==Difference::Same {result.rows[index].status=Difference::Changed;}
            }
            parent=p;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(txt:&str)->Registry{parse_reg(txt.as_bytes()).unwrap()}
    const HEAD:&str="Windows Registry Editor Version 5.00\n\n";
    #[test] fn equal_case_insensitive_names(){let a=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\Soft] \n\"Title\"=\"A\"\n"));let b=s(&(HEAD.to_owned()+"[hkey_current_user\\soft]\n\"title\"=\"A\"\n"));assert!(compare(&a,&b).rows.iter().all(|r|r.status==Difference::Same));}
    #[test] fn detects_types_and_orphans(){let a=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"A\"=dword:00000001\n\"B\"=hex:00,01\n"));let b=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"A\"=\"1\"\n"));let diff=compare(&a,&b);assert_eq!(diff.changed_count(),1);assert!(diff.orphan_count()>0);}
    #[test] fn extended_types(){let a=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n@=\"hello\"\n\"E\"=hex(2):41,00,00,00\n\"M\"=hex(7):41,00,00,00,00,00\n\"Q\"=hex(b):08,07,06,05,04,03,02,01\n"));let v=&a.keys["hkey_current_user\\x"].values;assert_eq!(v["q"].type_id,11);assert_eq!(v["e"].displayed(),"A");assert_eq!(v["m"].displayed(),"A");assert!(v.contains_key(""));}
    #[test] fn utf16_file(){let text=HEAD.to_owned()+"[HKEY_CURRENT_USER\\Café]\n\"texto\"=\"São Paulo\"\n";let mut b=vec![255,254];for c in text.encode_utf16(){b.extend(c.to_le_bytes());}let r=parse_reg(&b).unwrap();assert!(r.keys.contains_key("hkey_current_user\\café"));}
    #[test] fn reject_patches(){assert!(parse_reg((HEAD.to_owned()+"[-HKEY_CURRENT_USER\\X]").as_bytes()).is_err());assert!(parse_reg((HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"A\"=-").as_bytes()).is_err());}
    #[test] fn reject_conflict(){assert!(parse_reg((HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"A\"=dword:00000000\n\"a\"=dword:00000001").as_bytes()).is_err());}
    #[test] fn parents_propagate(){let a=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\A\\B] \n\"X\"=\"a\"\n"));let b=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\A\\B] \n\"X\"=\"b\"\n"));assert_eq!(compare(&a,&b).rows.iter().find(|r|r.path.eq_ignore_ascii_case("hkey_current_user\\a") && r.item==ItemKind::Key).unwrap().status,Difference::Changed);}
    #[test] fn line_continuation(){let x=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"B\"=hex:01,02,\\\n  03,04\n"));assert_eq!(x.keys["hkey_current_user\\x"].values["b"].bytes,[1,2,3,4]);}
    #[test] fn unknown_type_preserved(){let x=s(&(HEAD.to_owned()+"[HKEY_CURRENT_USER\\X]\n\"U\"=hex(42):de,ad\n"));assert_eq!(x.keys["hkey_current_user\\x"].values["u"].type_id,0x42);}
}

#[cfg(test)]
mod registry_edge_tests {
    use super::*;
    #[test]
    fn slash_in_value_name_is_not_a_parent_key() {
        let h="Windows Registry Editor Version 5.00\n";
        let a=format!("{h}[HKEY_CURRENT_USER\\A]\n\"A\\\\B\"=dword:00000001\n");
        let b=format!("{h}[HKEY_CURRENT_USER\\A]\n\"A\\\\B\"=dword:00000002\n");
        let result=compare(&parse_reg(a.as_bytes()).unwrap(),&parse_reg(b.as_bytes()).unwrap());
        assert_eq!(result.changed_count(),1);
        let row=result.rows.iter().find(|r|r.item==ItemKind::Value).unwrap();
        assert_eq!(row.key_path,"HKEY_CURRENT_USER\\A");
        assert_eq!(row.name,"A\\B");
    }
    #[test]
    fn type_id_is_not_inferred_from_display_text() {
        let h="Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\A]\n";
        let a=parse_reg(format!("{h}\"Test\"=\"42\"\n").as_bytes()).unwrap();
        let b=parse_reg(format!("{h}\"Test\"=hex(2):34,00,32,00,00,00\n").as_bytes()).unwrap();
        let result=compare(&a,&b);
        assert_eq!(result.changed_count(),1);
    }
    #[test]
    #[cfg(not(windows))]
    fn unknown_encodings_are_not_guessed() {
        assert!(parse_reg(b"REGEDIT4\n[HKEY_CURRENT_USER\\X]\n\"A\"=\"\xE1\"\n").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn regedit4_uses_windows_ansi_but_v5_without_bom_rejects_invalid_utf8() {
        assert!(parse_reg(b"REGEDIT4\n[HKEY_CURRENT_USER\\X]\n\"A\"=\"\xE1\"\n").is_ok());
        assert!(parse_reg(b"Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\X]\n\"A\"=\"\xE1\"\n").is_err());
    }
    #[test]
    fn invalid_roots_rejected() {
        assert!(parse_reg(b"Windows Registry Editor Version 5.00\n[HKEY_FAKE\\A]\n").is_err());
    }
}

#[cfg(test)]
mod regedit4_codepage_tests {
    #[cfg(windows)]
    #[test]
    fn regedit4_ascii_uses_system_ansi_path() {
        let file=b"REGEDIT4\n[HKEY_CURRENT_USER\\Demo]\n\"Value\"=dword:00000001\n";
        let reg=super::parse_reg(file).unwrap();
        assert_eq!(reg.keys["hkey_current_user\\demo"].values["value"].bytes,[1,0,0,0]);
    }
    #[cfg(not(windows))]
    #[test]
    fn ansi_without_known_codepage_is_rejected_portably() {
        let file=b"REGEDIT4\n[HKEY_CURRENT_USER\\Demo]\n\"Value\"=\"\xe9\"\n";
        assert!(super::parse_reg(file).is_err());
    }
}
