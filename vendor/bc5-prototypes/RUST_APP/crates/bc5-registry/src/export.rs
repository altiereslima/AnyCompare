//! Serialização portátil de snapshots `.reg`, sem escrever no Registro.
//! Formato válido para importação Windows, não a formatação A/B oficial do BC5.
use super::{canonical, Registry, RegValue, MAX_REG_BYTES};
use super::edit::validate_snapshot;

fn escape(s: &str) -> String { s.replace('\\', "\\\\").replace('"', "\\\"") }
fn render_value(v: &RegValue) -> String {
    let head = if v.name.is_empty() {"@".into()}
        else {format!("\"{}\"",escape(&v.name))};
    if v.type_id == 4 && v.bytes.len() == 4 {
        let val = u32::from_le_bytes([v.bytes[0],v.bytes[1],v.bytes[2],v.bytes[3]]);
        return format!("{head}=dword:{val:08x}\r\n");
    }
    let kind = if v.type_id==3 {"hex:".to_string()}
        else {format!("hex({:x}):",v.type_id)};
    let mut line = format!("{head}={kind}");
    for (index, byte) in v.bytes.iter().enumerate() {
        if index!=0 {
            if index%16==0 {line.push_str(",\\\r\n  ");}
            else {line.push(',');}
        }
        line.push_str(&format!("{byte:02x}"));
    }
    line.push_str("\r\n");
    line
}
fn encode(text: &str) -> Result<Vec<u8>, String> {
    if text.len()>MAX_REG_BYTES {return Err("exportação .reg excede limite de tamanho".into());}
    let mut out=vec![0xff,0xfe];
    for word in text.encode_utf16() {
        if out.len()+2>MAX_REG_BYTES {return Err("exportação UTF-16 excede 64 MiB".into());}
        out.extend_from_slice(&word.to_le_bytes());
    }
    Ok(out)
}
fn serialize(reg:&Registry, base:Option<&str>) -> Result<Vec<u8>,String> {
    // O exportador não pode emitir caminhos/valores com mapa interno incoerente.
    // Importação ou manipulação malformada deve falhar antes de criar arquivo.
    validate_snapshot(reg)?;
    let selected = base.map(canonical);
    // Antes de montar a saída, recusar entradas que necessariamente excedem
    // o teto: cada byte binário hex corresponde a pelo menos quatro bytes
    // UTF-16LE (dois algarismos x dois bytes). Contar apenas as chaves a
    // exportar; uma subárvore pequena não depende do tamanho de outra.
    let mut raw_hex_bytes = 0usize;
    for (id, key) in &reg.keys {
        if let Some(prefix) = &selected {
            if id != prefix && !id.starts_with(&format!("{prefix}\\")) { continue; }
        }
        for value in key.values.values() {
            if value.type_id == 4 && value.bytes.len() == 4 { continue; }
            raw_hex_bytes = raw_hex_bytes.checked_add(value.bytes.len())
                .ok_or("tamanho de exportação excedido")?;
            if raw_hex_bytes > MAX_REG_BYTES / 4 {
                return Err("dados brutos não cabem no limite de exportação .reg".into());
            }
        }
    }
    if let Some(prefix)=&selected {
        if !reg.keys.contains_key(prefix) {return Err("chave para exportação não encontrada".into());}
    }
    let mut text=String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    for (canon, key) in &reg.keys {
        // Nomes nativos podem conter CR/LF e não são representáveis com
        // segurança neste serializador de linhas .reg. Falhar sem perda.
        if key.path.chars().any(|c| matches!(c,'\r'|'\n'|'\0')) || key.values.values().any(|v|v.name.chars().any(|c| matches!(c,'\r'|'\n'|'\0'))) {
            return Err("nome de Registro contém controle não representável em .reg".into());
        }
        if let Some(prefix)=&selected {
            if canon!=prefix && !canon.starts_with(&format!("{prefix}\\")){continue;}
        }
        text.push_str(&format!("[{}]\r\n",key.path));
        for value in key.values.values() {text.push_str(&render_value(value));}
        text.push_str("\r\n");
        if text.len()>MAX_REG_BYTES {return Err("exportação muito grande".into());}
    }
    encode(&text)
}
/// Gera .reg UTF-16LE com BOM sem alterar o Registro nem arquivos existentes.
pub fn export_reg(reg:&Registry) -> Result<Vec<u8>,String> {serialize(reg,None)}
/// Exporta a chave e suas subchaves; preserva tipos e bytes sem normalizá-los.
pub fn export_subtree(reg:&Registry, base:&str) -> Result<Vec<u8>,String> {
    serialize(reg,Some(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compare, parse_reg, Difference};
    #[test]
    fn export_round_trip_preserves_type_bytes_unicode_and_default() {
        let src="Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Olá]\n@=\"padrão\"\n\"UTF\\\\16\"=hex(2):e7,00,00,00\n\"Num\"=dword:fedcba98\n\"Data\"=hex(42):de,ad\n";
        let reg=parse_reg(src.as_bytes()).unwrap();
        let output=export_reg(&reg).unwrap();
        assert!(output.starts_with(&[0xff,0xfe]));
        let recovered=parse_reg(&output).unwrap();
        assert!(compare(&reg,&recovered).rows.iter().all(|r|r.status==Difference::Same));
    }
    #[test]
    fn subtree_export_is_filtered_not_a_patch() {
        let r=parse_reg(b"Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\A]\n\"X\"=dword:00000001\n[HKEY_CURRENT_USER\\B]\n\"Y\"=dword:00000002\n").unwrap();
        let b=export_subtree(&r,"HKEY_CURRENT_USER\\A").unwrap();
        let read=parse_reg(&b).unwrap();
        assert!(read.keys.contains_key("hkey_current_user\\a"));
        assert!(!read.keys.contains_key("hkey_current_user\\b"));
        assert!(export_subtree(&r,"HKEY_CURRENT_USER\\nada").is_err());
    }
    #[test]
    fn repeat_case_with_same_data_is_not_false_conflict() {
        let r=parse_reg(b"Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\A]\n\"Name\"=dword:00000001\n\"name\"=dword:00000001\n").unwrap();
        assert_eq!(r.keys["hkey_current_user\\a"].values.len(),1);
    }
    #[test]
    fn fail_closed_on_unrepresentable_value_name() {
        let mut registry=Registry::default();
        let mut values=std::collections::BTreeMap::new();
        values.insert("line\nfeed".into(),RegValue{name:"line\nfeed".into(),type_id:3,bytes:vec![1]});
        registry.keys.insert("hkey_current_user".into(),crate::RegKey{path:"HKEY_CURRENT_USER".into(),values});
        assert!(export_reg(&registry).is_err());
    }

}
