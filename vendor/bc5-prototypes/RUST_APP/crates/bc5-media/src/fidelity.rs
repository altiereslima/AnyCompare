//! Diagnóstico de hipóteses de igualdade: NÃO é a política do BC5.
//!
//! Os registros da observação bruta carregam fonte, ID de frame/átomo,
//! ocorrências, offsets e hash dos bytes. Três políticas plausíveis dão
//! respostas diferentes quando ordem ou multiplicidade mudam. Até que uma
//! captura REAL do BC5 resolva a dúvida, o aplicativo não deve escolher
//! silenciosamente uma delas como regra original.
use crate::forensic::RawMediaObservation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateField {
    pub source: String,
    pub raw_key: String,
    pub left_occurrences: usize,
    pub right_occurrences: usize,
    /// Preserva a ordem das ocorrências e a multiplicidade dos valores.
    pub ordered_equal: bool,
    /// Ignora apenas a ordem; NÃO elimina valores duplicados.
    pub multiset_equal: bool,
    /// Ignora ordem E duplicatas; pode esconder diferenças reais.
    pub set_equal: bool,
    /// Pelo menos duas hipóteses levam a decisões diferentes.
    pub policy_sensitive: bool,
    /// Offsets são proveniência de cada arquivo, nunca critério de igualdade.
    pub left_offsets: Vec<u64>,
    pub right_offsets: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateReport {
    pub left_file: String,
    pub right_file: String,
    pub fields: Vec<CandidateField>,
    pub policy_sensitive_fields: usize,
    pub matches_bc5: Option<bool>,
    pub warning: String,
}

/// Compara registros RAW por fonte e chave. Não une TYER com TDRC,
/// ID3v1 com ID3v2, nem versões ID3 diferentes.
/// Não interpreta diferenças de bytes como importância BC5.
///
/// A origem (src, key) é uma classe de investigação, não uma afirmação
/// de que o BC5 agrupe exatamente esses campos em sua interface.
pub fn compare_candidate_policies(left: &RawMediaObservation, right: &RawMediaObservation) -> CandidateReport {
    type Group = (String, String);
    type Values = Vec<(String, u64)>;
    fn collect(input: &RawMediaObservation) -> BTreeMap<Group, Values> {
        let mut grouped: BTreeMap<Group, Values> = BTreeMap::new();
        for record in &input.records {
            grouped.entry((record.source.clone(), record.key.clone()))
                .or_default().push((record.blake3.clone(), record.offset));
        }
        grouped
    }
    let l = collect(left);
    let r = collect(right);
    let keys: BTreeSet<Group> = l.keys().chain(r.keys()).cloned().collect();
    let mut fields = Vec::with_capacity(keys.len());
    for (source, raw_key) in keys {
        let left_records = l.get(&(source.clone(), raw_key.clone()));
        let right_records = r.get(&(source.clone(), raw_key.clone()));
        let lhs: Vec<&str> = left_records.into_iter().flatten().map(|x| x.0.as_str()).collect();
        let rhs: Vec<&str> = right_records.into_iter().flatten().map(|x| x.0.as_str()).collect();
        let left_offsets = left_records.into_iter().flatten().map(|x| x.1).collect();
        let right_offsets = right_records.into_iter().flatten().map(|x| x.1).collect();
        let ordered_equal = lhs == rhs;
        let mut sorted_l = lhs.clone(); let mut sorted_r = rhs.clone();
        sorted_l.sort_unstable(); sorted_r.sort_unstable();
        let multiset_equal = sorted_l == sorted_r;
        sorted_l.dedup(); sorted_r.dedup();
        let set_equal = sorted_l == sorted_r;
        let policy_sensitive = ordered_equal != multiset_equal || multiset_equal != set_equal;
        fields.push(CandidateField {
            source, raw_key,
            left_occurrences: lhs.len(), right_occurrences: rhs.len(),
            ordered_equal, multiset_equal, set_equal, policy_sensitive,
            left_offsets, right_offsets,
        });
    }
    let policy_sensitive_fields = fields.iter().filter(|f| f.policy_sensitive).count();
    CandidateReport {
        left_file: left.file.clone(), right_file: right.file.clone(),
        fields, policy_sensitive_fields, matches_bc5: None,
        warning: "Hipóteses de investigação, NÃO resultado do BC5; prioridade, agrupamento de tags, normalização e ordem do programa original não validados A/B".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forensic::RawMediaRecord;
    fn sample(name: &str, records: &[(&str, &str, &str)]) -> RawMediaObservation {
        let mut rows = Vec::new();
        for (i, (src, key, value)) in records.iter().enumerate() {
            rows.push(RawMediaRecord {
                source: (*src).into(), key: (*key).into(), occurrence: i,
                offset: (i*100) as u64, value_length: value.len(),
                blake3: blake3::hash(value.as_bytes()).to_hex().to_string(),
                container_path: None, meta_header_offset: None, id3_header_offset: None,
                utf8_value: Some((*value).into()), hex_prefix: String::new(),
            });
        }
        let json = serde_json::json!({
            "file": name, "file_size": 0, "container": "fixture", "records": rows,
            "warnings": [], "is_bc5_comparison": false
        });
        serde_json::from_value(json).unwrap()
    }
    #[test]
    fn reordering_is_distinct_from_missing_duplicates() {
        let l = sample("L", &[("vorbis", "ARTIST", "Ana"), ("vorbis", "ARTIST", "Bia")]);
        let r = sample("R", &[("vorbis", "ARTIST", "Bia"), ("vorbis", "ARTIST", "Ana")]);
        let row = &compare_candidate_policies(&l, &r).fields[0];
        assert!(!row.ordered_equal);
        assert!(row.multiset_equal && row.set_equal && row.policy_sensitive);
    }
    #[test]
    fn missing_duplicate_is_not_equal_under_multiset() {
        let l = sample("L", &[("vorbis", "ARTIST", "Ana"), ("vorbis", "ARTIST", "Ana")]);
        let r = sample("R", &[("vorbis", "ARTIST", "Ana")]);
        let c = compare_candidate_policies(&l, &r);
        let row = &c.fields[0];
        assert!(!row.ordered_equal && !row.multiset_equal && row.set_equal);
        assert_eq!(c.policy_sensitive_fields, 1);
        assert!(c.matches_bc5.is_none());
    }
    #[test]
    fn different_id3_versions_never_merge() {
        let l = sample("L", &[("id3v2.3", "TYER", "1999")]);
        let r = sample("R", &[("id3v2.4", "TDRC", "1999")]);
        let c = compare_candidate_policies(&l, &r);
        assert_eq!(c.fields.len(), 2);
        assert!(c.fields.iter().all(|f| !f.set_equal));
    }
    #[test]
    fn unrelated_offsets_are_not_semantic_differences() {
        let l = sample("L", &[("id3v2.3", "TIT2", "Same")]);
        let mut r = sample("R", &[("id3v2.3", "TIT2", "Same")]);
        r.records[0].offset = 4096;
        let c = compare_candidate_policies(&l, &r);
        assert!(c.fields[0].ordered_equal);
        assert_ne!(c.fields[0].left_offsets, c.fields[0].right_offsets);
    }
    #[test]
    fn preserves_utf8_bytes_without_assuming_case_rules() {
        let l = sample("L", &[("vorbis", "TITLE", "Árvore")]);
        let r = sample("R", &[("vorbis", "TITLE", "árvore")]);
        let c = compare_candidate_policies(&l, &r);
        assert!(!c.fields[0].set_equal);
    }
}
