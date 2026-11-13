//! Observador byte a byte para investigação de tags de mídia.
//!
//! IMPORTANTE: não reproduz o algoritmo de comparação do BC5. Preserva
//! identificadores, ordem, ocorrências, offsets e bytes para contrastar com
//! relatórios reais produzidos pelo BC5 5.2.7.32913. Não escreve na origem.
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;

const MAX_INPUT: u64 = 256 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;
const MAX_DEPTH: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMediaRecord {
    pub source: String,
    /// Identificador original, e.g. `TIT2`, `TITLE`, `©nam` (UTF-8 when possible).
    pub key: String,
    /// Número da ocorrência dentro desta origem (inicia em zero).
    pub occurrence: usize,
    pub offset: u64,
    pub value_length: usize,
    pub blake3: String,
    /// Caminho do recipiente MP4 de origem, sem fingir que todos os `ilst`
    /// pertencem à mesma tabela `keys`. Opcional em snapshots legados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_path: Option<String>,
    /// Endereço do CABEÇALHO do átomo `meta` proprietário deste registro.
    /// Diferencia recipientes irmãos com caminho textual idêntico.
    /// Opcional para preservar snapshots das etapas anteriores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_header_offset: Option<u64>,
    /// Offset absoluto do cabeçalho ID3v2 que contém este registro.
    /// Permite interpretar flags de unsynchronisation por tag, inclusive quando
    /// há duas tags ID3v2 consecutivas; snapshots antigos continuam legíveis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id3_header_offset: Option<u64>,
    /// Conteúdo somente quando os bytes originais são UTF-8 válido.
    pub utf8_value: Option<String>,
    /// Prefixo dos bytes originais, não decodifica nem altera a entrada.
    pub hex_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMediaObservation {
    pub file: String,
    pub file_size: u64,
    /// Hash de todo o arquivo, não apenas dos trechos cobertos por tags.
    /// Observações antigas sem este campo continuam legíveis, com alerta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_blake3: Option<String>,
    pub container: String,
    pub records: Vec<RawMediaRecord>,
    pub warnings: Vec<String>,
    pub is_bc5_comparison: bool,
    #[serde(skip)]
    occurrence_counts: HashMap<(String,String),usize>,
}

impl RawMediaObservation {
    fn push(&mut self, src: &str, key: &str, start: usize, bytes: &[u8]) {
        self.push_with_meta(src,key,start,bytes,None,None);
    }

    fn push_with_meta(&mut self, src: &str, key: &str, start: usize,
                      bytes: &[u8], path: Option<&str>, meta_offset: Option<usize>) {
        if self.records.len() >= MAX_RECORDS {
            if !self.warnings.iter().any(|w| w == "limite de registros atingido") {
                self.warnings.push("limite de registros atingido".to_string());
            }
            return;
        }
        let counter = self.occurrence_counts.entry((src.to_owned(),key.to_owned())).or_insert(0);
        let occurrence = *counter;
        *counter += 1;
        let hex_prefix: String = bytes.iter().take(24).map(|b| format!("{b:02x}")).collect();
        self.records.push(RawMediaRecord {
            source: src.to_string(), key: key.to_string(), occurrence,
            offset: start as u64, value_length: bytes.len(),
            blake3: blake3::hash(bytes).to_hex().to_string(),
            container_path: path.map(ToOwned::to_owned),
            meta_header_offset: meta_offset.map(|x|x as u64),
            id3_header_offset: None,
            utf8_value: std::str::from_utf8(bytes).ok().map(ToOwned::to_owned),
            hex_prefix,
        });
    }

    fn push_id3(&mut self, src:&str, key:&str, offset:usize, bytes:&[u8], header_offset:usize) {
        let previous=self.records.len();
        self.push(src,key,offset,bytes);
        if self.records.len()>previous {
            self.records[previous].id3_header_offset=Some(header_offset as u64);
        }
    }
}

fn be32(v: &[u8]) -> Option<usize> {
    if v.len() < 4 { None } else { Some(u32::from_be_bytes(v[0..4].try_into().ok()?) as usize) }
}
fn le32(v: &[u8]) -> Option<usize> {
    if v.len() < 4 { None } else { Some(u32::from_le_bytes(v[0..4].try_into().ok()?) as usize) }
}
fn synchsafe(v: &[u8]) -> Option<usize> {
    if v.len() < 4 || v[..4].iter().any(|b| b & 0x80 != 0) { return None; }
    Some(((v[0] as usize) << 21) | ((v[1] as usize) << 14) | ((v[2] as usize) << 7) | v[3] as usize)
}

/// Confere o footer ID3v2.4 antes de avançar para o próximo bloco.
/// Um footer ausente/malformado NÃO autoriza pular os 10 bytes seguintes.
fn id3_footer_end(buf:&[u8], base:usize, body_end:usize, major:u8,
                  source:&str, out:&mut RawMediaObservation) -> usize {
    if major!=4 || buf[base+5] & 0x10==0 {return body_end;}
    let Some(footer_end)=body_end.checked_add(10).filter(|x|*x<=buf.len()) else {
        out.warnings.push(format!("ID3v2.4 em {base}: footer declarado mas ausente/truncado"));
        return body_end;
    };
    let footer=&buf[body_end..footer_end];
    if footer.starts_with(b"3DI") && footer[3..10]==buf[base+3..base+10] {
        out.push_id3(source,"FOOTER",body_end,footer,base);
        footer_end
    } else {
        out.warnings.push(format!("ID3v2.4 em {base}: footer inválido (assinatura, versão, flags ou tamanho)"));
        body_end
    }
}

/// Observa uma tag ID3v2 com endereçamento absoluto. Não tenta resolver
/// precedência ID3v1/v2 nem estabelece igualdade do Beyond Compare.
fn scan_id3v2_at(buf:&[u8], base:usize, out:&mut RawMediaObservation) -> Option<usize> {
    let header=buf.get(base..base.checked_add(10)?)?;
    if &header[..3]!=b"ID3" {return None;}
    let major=header[3];
    if !(2..=4).contains(&major) {
        out.warnings.push(format!("ID3v2.{major} em {base}: versão não reconhecida"));
        return None;
    }
    let Some(size)=synchsafe(&header[6..10]) else {
        out.warnings.push(format!("ID3v2.{major} em {base}: tamanho inválido"));
        return None;
    };
    let Some(end)=base.checked_add(10).and_then(|v|v.checked_add(size))
                        .filter(|e|*e<=buf.len()) else {
        out.warnings.push(format!("ID3v2.{major} em {base}: tag truncada"));
        return None;
    };
    let source=format!("id3v2.{major}");
    // Reserved ID3 header bits are diagnostic anomalies, not BC5 rules.
    // ID3v2.2 permits bits 7/6; ID3v2.3 permits 7/6/5; v2.4 permits 7/6/5/4.
    let reserved_mask=match major { 2=>0x3f, 3=>0x1f, 4=>0x0f, _=>0 };
    if header[5] & reserved_mask != 0 {
        out.warnings.push(format!("ID3v2.{major} em {base}: bits reservados no header: 0x{:02x}",header[5] & reserved_mask));
    }
    out.push_id3(&source,"HEADER",base,header,base);
    if header[5] & 0x80 != 0 {
        out.warnings.push(format!("ID3v2.{major} em {base}: unsynchronisation global; conteúdo dos frames preservado RAW"));
    }
    if major==2 && header[5] & 0x40 != 0 {
        out.warnings.push(format!("ID3v2.2 em {base}: tag comprimida, frames não interpretados"));
        return Some(end);
    }
    let mut pos=base+10;
    if major>=3 && header[5] & 0x40 != 0 {
        if pos+4>end {
            out.warnings.push(format!("ID3v2.{major} em {base}: extended header truncado"));
            return Some(id3_footer_end(buf,base,end,major,&source,out));
        }
        let ext_len=if major==4 {synchsafe(&buf[pos..pos+4])} else {be32(&buf[pos..pos+4])};
        let Some(ext_len)=ext_len else {
            out.warnings.push(format!("ID3v2.{major} em {base}: extended header inválido"));
            return Some(id3_footer_end(buf,base,end,major,&source,out));
        };
        // ID3v2.3: tamanho EXCLUI os 4 bytes do próprio tamanho.
        // ID3v2.4: tamanho INCLUI os 4 bytes do próprio tamanho.
        let consumed=if major==3 {ext_len.checked_add(4)} else {Some(ext_len)};
        let Some(consumed)=consumed.filter(|n|*n>=if major==3 {10} else {6}) else {
            out.warnings.push(format!("ID3v2.{major} em {base}: extended header menor que o mínimo da especificação"));
            return Some(id3_footer_end(buf,base,end,major,&source,out));
        };
        if pos.checked_add(consumed).map_or(true,|x|x>end) {
            out.warnings.push(format!("ID3v2.{major} em {base}: extended header fora da tag"));
            return Some(id3_footer_end(buf,base,end,major,&source,out));
        }
        out.push_id3(&source,"EXTENDED_HEADER",pos,&buf[pos..pos+consumed],base);
        pos+=consumed;
    }
    while pos<end && out.records.len()<MAX_RECORDS {
        let header_size=if major==2 {6} else {10};
        if pos.checked_add(header_size).map_or(true,|x|x>end) {
            // Não confundir um cabeçalho parcial não nulo com padding ID3.
            if buf[pos..end].iter().any(|byte| *byte != 0) {
                out.warnings.push(format!("ID3v2.{major} em {base}: frame com cabeçalho parcial em {pos}"));
            }
            break;
        }
        let id_size=if major==2 {3} else {4};
        let id=&buf[pos..pos+id_size];
        if id.iter().all(|b|*b==0) {break;}
        if !id.iter().all(|b|b.is_ascii_uppercase() || b.is_ascii_digit()) {
            out.warnings.push(format!("ID3v2.{major} em {base}: frame ID inválido em {pos}"));
            break;
        }
        let size=if major==2 {
            ((buf[pos+3] as usize)<<16)|((buf[pos+4] as usize)<<8)|(buf[pos+5] as usize)
        } else if major==4 {
            let Some(n)=synchsafe(&buf[pos+4..pos+8]) else {
                out.warnings.push(format!("ID3v2.4 em {base}: frame com tamanho inválido em {pos}"));
                break;
            };n
        } else {be32(&buf[pos+4..pos+8]).unwrap_or(0)};
        let value_start=pos+header_size;
        let Some(next)=value_start.checked_add(size).filter(|x|*x<=end) else {
            out.warnings.push(format!("ID3v2.{major} em {base}: frame fora dos limites em {pos}"));
            break;
        };
        out.push_id3(&source,&String::from_utf8_lossy(id),value_start,&buf[value_start..next],base);
        pos=next;
    }
    Some(id3_footer_end(buf,base,end,major,&source,out))
}

/// Suporta várias tags ID3v2 consecutivas sem fundir origens/ocorrências.
/// Limite explícito evita abuso por cabeçalhos vazios em arquivo hostil.
fn scan_id3v2(buf:&[u8],out:&mut RawMediaObservation)->usize {
    let mut offset=0usize;
    for _ in 0..16 {
        let Some(next)=scan_id3v2_at(buf,offset,out) else {return offset;};
        if next<=offset {break;}
        offset=next;
    }
    if buf.get(offset..offset.saturating_add(3))==Some(b"ID3") {
        out.warnings.push("limite de 16 tags ID3v2 consecutivas atingido".into());
    }
    offset
}

fn scan_id3v1(buf: &[u8], out: &mut RawMediaObservation) {
    if buf.len() < 128 { return; }
    let p = buf.len()-128;
    if &buf[p..p+3] != b"TAG" {return;}
    for (key, start, len) in [
        ("TITLE",3,30), ("ARTIST",33,30), ("ALBUM",63,30),
        ("YEAR",93,4), ("COMMENT",97,30), ("GENRE",127,1),
    ] {
        out.push("id3v1",key,p+start,&buf[p+start..p+start+len]);
    }
    if buf[p+125] == 0 && buf[p+126] != 0 {out.push("id3v1", "TRACK",p+126,&buf[p+126..p+127]);}
}

fn scan_flac(buf: &[u8], start: usize, out: &mut RawMediaObservation) {
    if buf.get(start..start+4) != Some(b"fLaC") {return;}
    out.container="flac".into();
    let mut p=start+4;
    let mut blocks=0usize;
    let mut streaminfo_seen=false;
    let mut final_block_seen=false;
    while p+4<=buf.len() && out.records.len() < MAX_RECORDS {
        let block_type=buf[p]&0x7f;
        let last=buf[p]&0x80!=0;
        let size=((buf[p+1] as usize)<<16)|((buf[p+2] as usize)<<8)|buf[p+3] as usize;
        let body=p+4;
        let Some(end)=body.checked_add(size).filter(|e|*e<=buf.len()) else {out.warnings.push("bloco FLAC truncado".into());break;};
        blocks+=1;
        if blocks==1 && block_type!=0 {
            out.warnings.push("FLAC: primeiro bloco de metadados não é STREAMINFO".into());
        }
        match block_type {
            4 => scan_vorbis_comments(&buf[body..end],body,out),
            6 => out.push("flac", "PICTURE",body,&buf[body..end]),
            0 => {
                if streaminfo_seen { out.warnings.push("FLAC: STREAMINFO duplicado".into()); }
                if size!=34 { out.warnings.push(format!("FLAC: STREAMINFO tem {size} bytes, esperado 34")); }
                streaminfo_seen=true;
                out.push("flac", "STREAMINFO",body,&buf[body..end]);
            },
            127 => {
                out.warnings.push("FLAC: tipo de bloco 127 inválido/reservado".into());
                out.push("flac", "BLOCK_127",body,&buf[body..end]);
            },
            n => out.push("flac",&format!("BLOCK_{n}"),body,&buf[body..end]),
        }
        p=end;
        if last {final_block_seen=true;break;}
    }
    if blocks==0 {
        out.warnings.push("FLAC sem bloco de metadados completo".into());
    } else if !final_block_seen {
        out.warnings.push("FLAC: não foi encontrado bloco final de metadados".into());
    }
}

fn scan_vorbis_comments(buf:&[u8],start:usize,out:&mut RawMediaObservation) {
    if buf.len()<8 {out.warnings.push("Vorbis Comments truncados".into());return;}
    let Some(vendor_len)=le32(&buf[..4]) else{return;};
    let Some(mut p)=4usize.checked_add(vendor_len).filter(|x|x.saturating_add(4)<=buf.len()) else {out.warnings.push("tamanho vendor inválido".into());return;};
    out.push("vorbis", "VENDOR", start+4, &buf[4..p]);
    let Some(count)=le32(&buf[p..p+4]) else{return;};p+=4;
    if count>MAX_RECORDS {
        out.warnings.push(format!("Vorbis: {count} campos declarados excedem o limite forense de {MAX_RECORDS}"));
    }
    for _ in 0..count.min(MAX_RECORDS) {
        if p+4>buf.len() {out.warnings.push("lista Vorbis truncada".into());break;}
        let Some(n)=le32(&buf[p..p+4]) else {break;};p+=4;
        let Some(end)=p.checked_add(n).filter(|x|*x<=buf.len()) else {out.warnings.push("campo Vorbis truncado".into());break;};
        if let Some(i)=buf[p..end].iter().position(|c|*c==b'=') {
            let key=String::from_utf8_lossy(&buf[p..p+i]);
            out.push("vorbis",&key,start+p+i+1,&buf[p+i+1..end]);
        } else {out.push("vorbis","FIELD_WITHOUT_EQUALS",start+p,&buf[p..end]);}
        p=end;
    }
}

fn atom_display(s: &[u8]) -> String {
    // Os índices de ilst/mdta são IDs binários de 32 bits (ex. 00 00 00 01).
    // NUL é UTF-8 válido, mas NÃO deve entrar como chave textual invisível.
    if s.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return String::from_utf8_lossy(s).into_owned();
    }
    format!("0x{}", s.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Reconhecimento estrutural ISO BMFF, não detecção de suporte pelo BC5.
/// Permite átomos opcionais 'free', 'skip' e 'wide' antes de 'ftyp'; nunca
/// decide pelo nome da extensão nem encontra assinaturas dentro de 'mdat'.
fn has_mp4_ftyp(buf: &[u8]) -> bool {
    let mut p=0usize;
    for _ in 0..8 {
        if p.checked_add(8).map_or(true, |n| n>buf.len()) { return false; }
        let Some(size32)=be32(&buf[p..p+4]) else { return false; };
        let typ=&buf[p+4..p+8];
        let (size,header)=if size32==1 {
            let Some(raw)=buf.get(p+8..p+16) else {return false;};
            let size64=u64::from_be_bytes(raw.try_into().expect("8 bytes"));
            if size64>usize::MAX as u64 {return false;}
            (size64 as usize,16usize)
        } else if size32==0 {(buf.len()-p,8usize)} else {(size32,8usize)};
        if size<header || p.checked_add(size).map_or(true, |end|end>buf.len()) {return false;}
        if typ==&b"ftyp"[..] {return size>=header+8;}
        if typ!=&b"free"[..] && typ!=&b"skip"[..] && typ!=&b"wide"[..] {return false;}
        p+=size;
    }
    false
}
fn mp4_boxes(buf:&[u8], mut p:usize, end:usize, depth:usize, parent:&str,
             meta_header_offset:Option<usize>, out:&mut RawMediaObservation) {
    if depth>MAX_DEPTH {
        out.warnings.push(format!("MP4: limite forense de profundidade atingido em {parent}"));
        return;
    }
    while p.saturating_add(8)<=end && out.records.len()<MAX_RECORDS {
        let Some(mut n)=be32(&buf[p..p+4]) else {break;};
        let typ=&buf[p+4..p+8];
        let mut hdr=8usize;
        if n==1 {
            if p+16>end {out.warnings.push("MP4 extended size truncado".into());break;}
            let large=u64::from_be_bytes(buf[p+8..p+16].try_into().unwrap());
            if large>usize::MAX as u64 {break;}
            n=large as usize;hdr=16;
        }
        if n==0 {
            // Em ISO BMFF, tamanho zero significa até o fim do ARQUIVO,
            // não até o fim de um recipiente arbitrário. Não atribuímos
            // um tamanho sintético a um filho aninhado.
            if depth>0 {
                out.warnings.push(format!("MP4: átomo com tamanho zero dentro de {parent} em {p}"));
                break;
            }
            n=end-p;
        }
        let Some(next)=p.checked_add(n).filter(|x| *x<=end && *x<=buf.len() && *x>p && n>=hdr) else {
            out.warnings.push(format!("átomo MP4 inválido em {p}"));break;
        };
        let key=atom_display(typ);
        let name=format!("{parent}/{key}");
        let body=p+hdr;
        if parent.ends_with("/ilst") {
            // The ilst child is itself a metadata entry; inspect its child atoms.
            out.push_with_meta("mp4/ilst",&key,body,&buf[body..next],Some(parent),meta_header_offset);
            mp4_boxes(buf,body,next,depth+1,&name,meta_header_offset,out);
        } else if parent.contains("/ilst/") {
            // Preserve the whole data atom, including its 8-byte type/locale header.
            out.push_with_meta("mp4/ilst-data",&name,body,&buf[body..next],Some(parent),meta_header_offset);
        } else if typ == &b"Xtra"[..] || typ == &b"keys"[..] {
            // O EXE do BC5 contém RTTI de TMP4XtraPriority e
            // TMP4XtraUsageConversion. Isso justifica PRESERVAR estes bytes,
            // mas NÃO aplicar uma prioridade presumida entre Xtra e ilst.
            // 'keys' é a tabela de identificação de metadados MP4/mdta;
            // não inferimos a correspondência de índices sem testes BC5.
            out.push_with_meta("mp4/auxiliary", &name, body, &buf[body..next],Some(parent),meta_header_offset);
        } else if [b"moov",b"udta",b"trak",b"mdia",b"minf",b"stbl",b"ilst",b"meta"].iter().any(|x|typ==x.as_slice()) {
            let child_start=if typ==&b"meta"[..] {
                if next-body < 4 {
                    out.warnings.push(format!("átomo MP4 meta truncado em {p}: faltam os 4 bytes de versão/flags"));
                    p=next;
                    continue;
                }
                body+4
            } else {body};
            mp4_boxes(buf,child_start,next,depth+1,&name,
                      if typ==&b"meta"[..] {Some(p)} else {meta_header_offset},out);
        } else if depth==0 && typ==&b"ftyp"[..] {
            out.push_with_meta("mp4", "ftyp",body,&buf[body..next],Some(parent),meta_header_offset);
        }
        p=next;
    }
    // O scanner anterior encerrava silenciosamente se restassem 1..7 bytes
    // dentro de um recipiente. São bytes que não formam um cabeçalho ISO BMFF.
    if p<end && out.records.len()<MAX_RECORDS {
        out.warnings.push(format!("MP4: {} byte(s) sem cabeçalho completo em {parent}, offset {p}",end-p));
    }
}

/// Lê até limite+1 bytes DO MESMO descritor aberto. A verificação apenas por
/// metadata seguida de fs::read deixava uma janela TOCTOU para arquivo crescente.
/// Esta rotina é uma proteção de diagnóstico, não parte do BC5 original.
fn read_bounded_with_limit(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let file=fs::File::open(path)?;
    if file.metadata()?.len()>limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData,"arquivo acima do limite forense"));
    }
    let mut data=Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut data)?;
    if data.len() as u64>limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData,"arquivo cresceu acima do limite durante a leitura"));
    }
    Ok(data)
}

pub(crate) fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    read_bounded_with_limit(path,MAX_INPUT)
}

/// Recria a observação canônica dos MESMOS bytes lidos pelo interpretador.
/// Usada para impedir injeção de registros forenses falsos via JSON.
pub(crate) fn observe_media_bytes(path: &Path, data: &[u8]) -> RawMediaObservation {
    let mut out=RawMediaObservation {
        file:path.to_string_lossy().into_owned(),file_size:data.len() as u64,
        file_blake3:Some(blake3::hash(data).to_hex().to_string()),
        container:"unknown".into(),records:Vec::new(), warnings:Vec::new(),
        is_bc5_comparison:false,
        occurrence_counts:HashMap::new(),
    };
    let audio_start=scan_id3v2(&data,&mut out);
    if data.get(audio_start..audio_start+4).map(|s|s==&b"fLaC"[..]).unwrap_or(false) {scan_flac(&data,audio_start,&mut out);}
    else if has_mp4_ftyp(&data) {out.container="mp4".into();mp4_boxes(&data,0,data.len(),0,"mp4",None,&mut out);}
    else if audio_start>0 || data.starts_with(&[0xff,0xfb]) || data.windows(3).take(100).any(|w|w==&[0xff,0xfb,0x90]) {out.container="mpeg-audio-possible".into();}
    scan_id3v1(&data,&mut out);
    if out.container=="unknown" && !out.records.is_empty(){out.container="tagged-unidentified".into();}
    if out.container=="unknown" {out.warnings.push("formato não identificado; sem suposição por extensão".into());}
    out
}

/// Coleta evidências sem transformar aliases de tags nem comparar valores.
/// Lê no máximo 256 MiB + 1 byte, mesmo se o arquivo crescer após stat().
pub fn observe_media_raw(path: impl AsRef<Path>) -> io::Result<RawMediaObservation> {
    let path=path.as_ref();
    let data=read_bounded(path)?;
    Ok(observe_media_bytes(path,&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_read_enforces_exact_limit_on_same_open_file() {
        use std::io::Write;
        let mut file=tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"1234567890").unwrap();
        file.flush().unwrap();
        assert_eq!(read_bounded_with_limit(file.path(),10).unwrap(),b"1234567890");
        assert_eq!(read_bounded_with_limit(file.path(),9).unwrap_err().kind(),io::ErrorKind::InvalidData);
        assert_eq!(read_bounded_with_limit(file.path(),0).unwrap_err().kind(),io::ErrorKind::InvalidData);
    }
    #[test]
    fn id3_versions_are_not_merged() {
        let mut src=vec![0u8;10+11+128];
        src[..10].copy_from_slice(b"ID3\x04\0\0\0\0\0\x0b");
        src[10..14].copy_from_slice(b"TIT2");
        src[14..18].copy_from_slice(&[0,0,0,1]);
        src[20]=0x41;
        let tail=src.len()-128;
        src[tail..tail+3].copy_from_slice(b"TAG");
        src[tail+3..tail+5].copy_from_slice(b"BC");
        let mut out=RawMediaObservation{file:String::new(),file_size:src.len() as u64,file_blake3:None,container:String::new(),records:vec![],warnings:vec![],is_bc5_comparison:false,occurrence_counts:HashMap::new()};
        scan_id3v2(&src,&mut out);scan_id3v1(&src,&mut out);
        assert!(out.records.iter().any(|r|r.source=="id3v2.4" && r.key=="TIT2"));
        assert!(out.records.iter().any(|r|r.source=="id3v1" && r.key=="TITLE"));
    }
    #[test]
    fn repeated_vorbis_fields_remain_repeated() {
        let mut bytes=vec![];
        bytes.extend(1u32.to_le_bytes());bytes.push(b'X');bytes.extend(2u32.to_le_bytes());
        for field in [b"ARTIST=A".as_slice(),b"ARTIST=A".as_slice()] {
            bytes.extend((field.len() as u32).to_le_bytes());bytes.extend(field);
        }
        let mut out=RawMediaObservation{file:String::new(),file_size:0,file_blake3:None,container:String::new(),records:vec![],warnings:vec![],is_bc5_comparison:false,occurrence_counts:HashMap::new()};
        scan_vorbis_comments(&bytes,0,&mut out);
        assert_eq!(out.records.iter().filter(|x|x.key=="ARTIST").count(),2);
        assert_eq!(out.records.iter().filter(|x|x.key=="ARTIST").map(|x|x.occurrence).collect::<Vec<_>>(),vec![0,1]);
    }
    #[test]
    fn mp4_xtra_and_keys_are_preserved_without_guessing_precedence() {
        // Minimal MP4 tree: moov/udta/{Xtra,meta/{keys,ilst}}.
        // Xtra and keys are evidence, not native BC5 comparison fields.
        fn atom(key: &[u8;4], payload: &[u8]) -> Vec<u8> {
            let mut result = ((payload.len()+8) as u32).to_be_bytes().to_vec();
            result.extend_from_slice(key);
            result.extend_from_slice(payload);
            result
        }
        let xtra=atom(b"Xtra",b"uninterpreted Xtra payload");
        let keys=atom(b"keys",b"raw keys content");
        let meta=atom(b"meta",&[&[0,0,0,0][..],&keys[..]].concat());
        let udta=atom(b"udta",&[&xtra[..],&meta[..]].concat());
        let moov=atom(b"moov",&udta);
        let mut out=RawMediaObservation{
            file:String::new(),file_size:moov.len() as u64,file_blake3:None,
            container:"mp4".into(),records:vec![],warnings:vec![],
            is_bc5_comparison:false,occurrence_counts:HashMap::new(),
        };
        mp4_boxes(&moov,0,moov.len(),0,"mp4",None,&mut out);
        assert!(out.records.iter().any(|r|r.key.ends_with("/Xtra") && r.value_length==26));
        assert!(out.records.iter().any(|r|r.key.ends_with("/keys") && r.value_length==16));
        assert!(!out.is_bc5_comparison);
    }

    #[test]
    fn mdta_indices_are_printable_hex_not_embedded_nul() {
        assert_eq!(atom_display(&[0,0,0,1]), "0x00000001");
        assert_eq!(atom_display(b"trkn"), "trkn");
        assert_eq!(atom_display(&[0xa9,b'n',b'a',b'm']), "0xa96e616d");
    }
    #[test]
    fn mp4_ftyp_after_free_atom_is_detected() {
        let mut bytes=vec![0,0,0,12,b'f',b'r',b'e',b'e',1,2,3,4];
        bytes.extend([0,0,0,16,b'f',b't',b'y',b'p',b'M',b'4',b'A',b' ',0,0,0,0]);
        assert!(has_mp4_ftyp(&bytes));
        assert!(!has_mp4_ftyp(b"arbitrary data with ftyp"));
    }
    #[test]
    fn mp4_ftyp_broken_headers_are_not_accepted() {
        assert!(!has_mp4_ftyp(&[0,0,0,4,b'f',b't',b'y',b'p']));
        assert!(!has_mp4_ftyp(&[0,0,0,8,b'f',b't',b'y',b'p']));
        assert!(!has_mp4_ftyp(&[0,0,0,0,b'f',b'r',b'e',b'e']));
    }
    #[test]
    fn truncated_mp4_meta_is_reported_not_silently_skipped() {
        let input=[0,0,0,10,b'm',b'e',b't',b'a',1,2];
        let mut out=RawMediaObservation{
            file:String::new(),file_size:input.len() as u64,file_blake3:None,
            container:"mp4".into(),records:vec![],warnings:vec![],
            is_bc5_comparison:false,occurrence_counts:HashMap::new(),
        };
        mp4_boxes(&input,0,input.len(),0,"mp4",None,&mut out);
        assert!(out.warnings.iter().any(|w|w.contains("meta truncado")));
    }
    #[test]
    fn synchsafe_rejects_invalid_high_bits() {
        assert_eq!(synchsafe(&[0,0,0,0x7f]),Some(127));
        assert_eq!(synchsafe(&[0x80,0,0,0]),None);
    }
}

/// Diferença no corpus bruto, sem aplicar normalização ou importância BC5.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawDifference {
    pub source: String,
    pub key: String,
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub identical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDifferenceReport {
    pub left_file: String,
    pub right_file: String,
    pub entries: Vec<RawDifference>,
    pub same: usize,
    pub different: usize,
    pub matches_bc5: Option<bool>,
    pub warning: String,
}

/// Compara *somente* hashes dos valores brutos, em ordem, por origem/ID.
/// Nunca decide se um campo é "importante" para o BC5.
pub fn compare_raw_observations(a: &RawMediaObservation, b: &RawMediaObservation) -> RawDifferenceReport {
    use std::collections::{BTreeMap, BTreeSet};
    let collect = |observation: &RawMediaObservation| -> BTreeMap<(String, String), Vec<String>> {
        let mut map: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for record in &observation.records {
            map.entry((record.source.clone(), record.key.clone()))
                .or_default().push(record.blake3.clone());
        }
        map
    };
    let left=collect(a);let right=collect(b);
    let keys:BTreeSet<(String,String)>=left.keys().chain(right.keys()).cloned().collect();
    let mut entries=Vec::new();let mut same=0;let mut different=0;
    for (source,key) in keys {
        let l=left.get(&(source.clone(),key.clone())).cloned().unwrap_or_default();
        let r=right.get(&(source.clone(),key.clone())).cloned().unwrap_or_default();
        let identical=l==r;
        if identical {same+=1;} else {different+=1;}
        entries.push(RawDifference{source,key,left:l,right:r,identical});
    }
    RawDifferenceReport{
        left_file:a.file.clone(),right_file:b.file.clone(),entries,same,different,
        matches_bc5:None,
        warning:"Diferenças RAW somente: não são o algoritmo, a apresentação ou as regras de importância do BC5".into(),
    }
}

#[cfg(test)]
mod forensic_comparison_tests {
    use super::*;
    #[test]
    fn duplicates_do_not_disappear_during_raw_comparison() {
        let mut a=RawMediaObservation{file:"L".into(),file_size:0,file_blake3:None,container:"flac".into(),records:vec![],warnings:vec![],is_bc5_comparison:false,occurrence_counts:HashMap::new()};
        let mut b=RawMediaObservation{file:"R".into(),file_size:0,file_blake3:None,container:"flac".into(),records:vec![],warnings:vec![],is_bc5_comparison:false,occurrence_counts:HashMap::new()};
        a.push("vorbis","ARTIST",1,b"Ana");a.push("vorbis","ARTIST",2,b"Ana");
        b.push("vorbis","ARTIST",1,b"Ana");
        let c=compare_raw_observations(&a,&b);
        assert_eq!(c.different,1);
        assert_eq!(c.entries[0].left.len(),2);
        assert_eq!(c.entries[0].right.len(),1);
        assert!(c.matches_bc5.is_none());
    }
}
