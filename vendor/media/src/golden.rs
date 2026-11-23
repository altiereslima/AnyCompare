//! Cadeia de custódia para observações A/B do Media Compare do BC5.
//!
//! NÃO decompila o algoritmo do BC5, NÃO interpreta automaticamente o relatório
//! original e NÃO certifica que um relatório foi produzido pelo executável.
//! Exige apenas que cada rótulo humano tenha arquivos de evidência existentes,
//! caminhos contidos na raiz e hashes SHA-256 calculados sobre os bytes reais.
//! Caso a evidência falhe, a observação é excluída da calibração.
use crate::calibration::{audit_by_field, CalibrationAudit, ObservedFieldCase};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceArtifact {
    /// Caminho RELATIVO à raiz da captura. Nunca pode escapar dela.
    pub relative_path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCase {
    pub case: ObservedFieldCase,
    /// Original BC5 report, não um resumo produzido pelo Rust.
    pub report: EvidenceArtifact,
    pub left: EvidenceArtifact,
    pub right: EvidenceArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceDecision {
    pub case_id: String,
    pub field_id: String,
    pub accepted: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenEvidenceAudit {
    pub total: usize,
    pub labeled: usize,
    pub verified_labeled: usize,
    pub unlabeled: usize,
    pub rejected: usize,
    pub decisions: Vec<EvidenceDecision>,
    pub hypotheses_from_verified_only: BTreeMap<String, CalibrationAudit>,
    /// A correspondência de hashes NÃO autentica o autor do relatório.
    pub proves_bc5_origin: bool,
    pub parity_1to1_proven: bool,
}

fn valid_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

fn safe_path(root: &Path, artifact: &EvidenceArtifact) -> io::Result<PathBuf> {
    if !valid_hex_sha256(&artifact.sha256) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SHA-256 ausente/inválido",
        ));
    }
    if artifact.relative_path.as_os_str().is_empty()
        || artifact
            .relative_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "caminho não relativo ou contém escape",
        ));
    }
    let path = root.join(&artifact.relative_path);
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "arquivo fora da raiz ou inválido",
        ));
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn attest(root: &Path, entry: &EvidenceCase) -> io::Result<()> {
    let required = entry
        .case
        .original_report_sha256
        .as_deref()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "rótulo sem hash do relatório original",
            )
        })?;
    if !required.eq_ignore_ascii_case(&entry.report.sha256) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "hash no caso diverge do manifesto",
        ));
    }
    let mut paths = Vec::new();
    for (name, artifact) in [
        ("relatório", &entry.report),
        ("esquerda", &entry.left),
        ("direita", &entry.right),
    ] {
        let path = safe_path(root, artifact)?;
        let actual = sha256_file(&path)?;
        if !actual.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("hash divergente: {name}"),
            ));
        }
        paths.push(path);
    }
    if paths[0] == paths[1] || paths[0] == paths[2] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relatório confundido com mídia de entrada",
        ));
    }
    Ok(())
}

/// Verifica TODOS os rótulos contra os arquivos originais, sem inferir igualdade.
/// Observações com `observed_equal = None` não entram na pontuação.
/// Um arquivo de referência alterado invalida somente seu caso, não a captura inteira.
pub fn audit_golden_evidence(
    root: &Path,
    entries: &[EvidenceCase],
) -> io::Result<GoldenEvidenceAudit> {
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "raiz não é diretório",
        ));
    }
    // Primeira passagem: conferir fisicamente CADA observação rotulada.
    // Inserir o identificador em `seen` antes dessa conferência permitia que
    // um relatório adulterado bloqueasse o relatório válido posterior.
    let mut decisions = Vec::with_capacity(entries.len());
    let mut labeled = 0;
    let mut unlabeled = 0;
    for entry in entries {
        let c = &entry.case;
        if c.observed_equal.is_none() {
            unlabeled += 1;
            decisions.push(EvidenceDecision {
                case_id: c.case_id.clone(),
                field_id: c.field_id.clone(),
                accepted: false,
                reason: "sem observação original; não pontuar".into(),
            });
            continue;
        }
        labeled += 1;
        let outcome = if c.case_id.trim().is_empty() || c.field_id.trim().is_empty() {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "caso/campo vazio",
            ))
        } else {
            attest(&root, entry)
        };
        decisions.push(match outcome {
            Ok(()) => EvidenceDecision {
                case_id: c.case_id.clone(),
                field_id: c.field_id.clone(),
                accepted: true,
                reason: "hashes reais conferidos; autoria BC5 não autenticada".into(),
            },
            Err(err) => EvidenceDecision {
                case_id: c.case_id.clone(),
                field_id: c.field_id.clone(),
                accepted: false,
                reason: err.to_string(),
            },
        });
    }

    // Segunda passagem: duas observações VÁLIDAS do mesmo caso/campo
    // são ambíguas, mesmo quando concordam. Rejeitar AMBAS impede que
    // a ordem de entrada ou um rótulo favorável escolha uma hipótese.
    let mut verified_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for decision in &decisions {
        if decision.accepted {
            *verified_counts
                .entry((decision.case_id.clone(), decision.field_id.clone()))
                .or_insert(0) += 1;
        }
    }
    for decision in &mut decisions {
        if decision.accepted
            && verified_counts
                .get(&(decision.case_id.clone(), decision.field_id.clone()))
                .copied()
                .unwrap_or(0)
                > 1
        {
            decision.accepted = false;
            decision.reason =
                "mais de uma observação válida para o mesmo caso/campo; todas excluídas".into();
        }
    }
    // O mesmo case_id identifica UMA captura (relatório, esquerda, direita).
    // Evidências individuais podem ter hashes corretos e, mesmo assim, rótulos
    // de campos de capturas DISTINTAS serem agregados sob um único case_id.
    // Isso contaminaria a calibragem. Rejeitar TODOS os rótulos do case_id
    // ambíguo, mesmo quando field_id é diferente.
    type CaptureIdentity = ((PathBuf, String), (PathBuf, String), (PathBuf, String));
    let mut captures: BTreeMap<String, BTreeSet<CaptureIdentity>> = BTreeMap::new();
    for (entry, decision) in entries.iter().zip(&decisions) {
        if !decision.accepted {
            continue;
        }
        let id = (
            (
                entry.report.relative_path.clone(),
                entry.report.sha256.to_ascii_lowercase(),
            ),
            (
                entry.left.relative_path.clone(),
                entry.left.sha256.to_ascii_lowercase(),
            ),
            (
                entry.right.relative_path.clone(),
                entry.right.sha256.to_ascii_lowercase(),
            ),
        );
        captures
            .entry(decision.case_id.clone())
            .or_default()
            .insert(id);
    }
    for decision in &mut decisions {
        if decision.accepted
            && captures
                .get(&decision.case_id)
                .map(|ids| ids.len() > 1)
                .unwrap_or(false)
        {
            decision.accepted = false;
            decision.reason =
                "case_id reúne capturas fisicamente distintas; rótulos excluídos".into();
        }
    }

    let verified: Vec<ObservedFieldCase> = entries
        .iter()
        .zip(&decisions)
        .filter(|(_, decision)| decision.accepted)
        .map(|(entry, _)| entry.case.clone())
        .collect();
    let rejected = labeled - verified.len();
    Ok(GoldenEvidenceAudit {
        total: entries.len(),
        labeled,
        verified_labeled: verified.len(),
        unlabeled,
        rejected,
        decisions,
        hypotheses_from_verified_only: audit_by_field(&verified),
        proves_bc5_origin: false,
        parity_1to1_proven: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact(rel: &str, bytes: &[u8]) -> EvidenceArtifact {
        EvidenceArtifact {
            relative_path: PathBuf::from(rel),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
    fn make_case() -> EvidenceCase {
        let report = artifact("evidence/report.txt", b"BC5 report fixture");
        EvidenceCase {
            case: ObservedFieldCase {
                case_id: "flac_case_1".into(),
                field_id: "tag.artist".into(),
                left: vec!["A".into()],
                right: vec!["B".into()],
                observed_equal: Some(false),
                original_report_sha256: Some(report.sha256.clone()),
            },
            report,
            left: artifact("media/a.flac", b"AAA"),
            right: artifact("media/b.flac", b"BBB"),
        }
    }
    fn setup() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("evidence")).unwrap();
        fs::create_dir_all(root.path().join("media")).unwrap();
        fs::write(
            root.path().join("evidence/report.txt"),
            b"BC5 report fixture",
        )
        .unwrap();
        fs::write(root.path().join("media/a.flac"), b"AAA").unwrap();
        fs::write(root.path().join("media/b.flac"), b"BBB").unwrap();
        root
    }
    #[test]
    fn verified_hashes_allow_falsification_but_never_prove_parity() {
        let dir = setup();
        let result = audit_golden_evidence(dir.path(), &[make_case()]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (1, 0));
        assert!(!result.parity_1to1_proven && !result.proves_bc5_origin);
    }
    #[test]
    fn report_mutation_rejects_case_without_scoring() {
        let dir = setup();
        fs::write(dir.path().join("evidence/report.txt"), b"changed").unwrap();
        let result = audit_golden_evidence(dir.path(), &[make_case()]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (0, 1));
        assert!(result.hypotheses_from_verified_only.is_empty());
    }
    #[test]
    fn changed_media_rejects_original_input() {
        let dir = setup();
        fs::write(dir.path().join("media/a.flac"), b"changed").unwrap();
        assert_eq!(
            audit_golden_evidence(dir.path(), &[make_case()])
                .unwrap()
                .rejected,
            1
        );
    }
    #[test]
    fn path_traversal_and_unsupported_absolute_paths_are_rejected() {
        let dir = setup();
        for bad in ["../outside.txt", "/etc/passwd"] {
            let mut c = make_case();
            c.report.relative_path = PathBuf::from(bad);
            assert_eq!(audit_golden_evidence(dir.path(), &[c]).unwrap().rejected, 1);
        }
    }
    #[test]
    fn unlabeled_placeholder_before_verified_case_does_not_block_the_case() {
        let dir = setup();
        let mut placeholder = make_case();
        placeholder.case.observed_equal = None;
        let result = audit_golden_evidence(dir.path(), &[placeholder, make_case()]).unwrap();
        assert_eq!(result.unlabeled, 1);
        assert_eq!(result.verified_labeled, 1);
        assert_eq!(result.rejected, 0);
    }

    #[test]
    fn blank_case_or_field_id_is_rejected_even_with_valid_hashes() {
        let dir = setup();
        let mut blank_case = make_case();
        blank_case.case.case_id = "   ".into();
        let mut blank_field = make_case();
        blank_field.case.field_id = "".into();
        let result = audit_golden_evidence(dir.path(), &[blank_case, blank_field]).unwrap();
        assert_eq!(result.verified_labeled, 0);
        assert_eq!(result.rejected, 2);
    }

    #[test]
    fn unlabeled_or_duplicate_case_never_scores_twice() {
        let dir = setup();
        let mut empty = make_case();
        empty.case.observed_equal = None;
        let result = audit_golden_evidence(dir.path(), &[make_case(), make_case(), empty]).unwrap();
        assert_eq!(result.verified_labeled, 0);
        assert_eq!(result.rejected, 2);
        assert_eq!(result.unlabeled, 1);
    }

    #[test]
    fn rejected_tampered_entry_does_not_poison_a_later_valid_entry() {
        let dir = setup();
        let mut forged = make_case();
        forged.report.sha256 = "0".repeat(64);
        // A versão anterior registrava (case_id,field_id) antes de atestar.
        let result = audit_golden_evidence(dir.path(), &[forged, make_case()]).unwrap();
        assert_eq!(result.verified_labeled, 1);
        assert_eq!(result.rejected, 1);
        assert!(!result.decisions[0].accepted);
        assert!(result.decisions[1].accepted);
    }

    #[test]
    fn two_valid_but_conflicting_labels_are_both_rejected() {
        let dir = setup();
        let mut conflicting = make_case();
        conflicting.case.observed_equal = Some(true);
        let result = audit_golden_evidence(dir.path(), &[make_case(), conflicting]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (0, 2));
        assert!(result.hypotheses_from_verified_only.is_empty());
        assert!(result.decisions.iter().all(|d| !d.accepted));
    }

    #[test]
    fn independent_fields_on_the_same_media_pair_are_allowed() {
        let dir = setup();
        let mut artist = make_case();
        artist.case.field_id = "tag.artist".into();
        let mut title = make_case();
        title.case.field_id = "tag.title".into();
        let result = audit_golden_evidence(dir.path(), &[artist, title]).unwrap();
        assert_eq!(result.verified_labeled, 2);
        assert_eq!(result.hypotheses_from_verified_only.len(), 2);
    }

    #[test]
    fn same_case_id_from_two_physically_distinct_captures_is_rejected() {
        let dir = setup();
        fs::write(
            dir.path().join("evidence/other.txt"),
            b"Second BC5 report fixture",
        )
        .unwrap();
        let mut title = make_case();
        title.case.field_id = "tag.title".into();
        title.report = artifact("evidence/other.txt", b"Second BC5 report fixture");
        title.case.original_report_sha256 = Some(title.report.sha256.clone());
        let result = audit_golden_evidence(dir.path(), &[make_case(), title]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (0, 2));
        assert!(result.decisions.iter().all(|d| !d.accepted));
    }

    #[test]
    fn distinct_case_ids_can_use_distinct_reports_on_same_files() {
        let dir = setup();
        fs::write(
            dir.path().join("evidence/other.txt"),
            b"Second BC5 report fixture",
        )
        .unwrap();
        let mut second = make_case();
        second.case.case_id = "flac_case_2".into();
        second.report = artifact("evidence/other.txt", b"Second BC5 report fixture");
        second.case.original_report_sha256 = Some(second.report.sha256.clone());
        let result = audit_golden_evidence(dir.path(), &[make_case(), second]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (2, 0));
    }

    #[test]
    fn same_case_id_with_reversed_media_inputs_is_ambiguous() {
        let dir = setup();
        let mut reversed = make_case();
        reversed.case.field_id = "tag.album".into();
        std::mem::swap(&mut reversed.left, &mut reversed.right);
        let result = audit_golden_evidence(dir.path(), &[make_case(), reversed]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (0, 2));
    }

    #[test]
    fn duplicate_valid_labels_never_double_count_even_if_identical() {
        let dir = setup();
        let result = audit_golden_evidence(dir.path(), &[make_case(), make_case()]).unwrap();
        assert_eq!((result.verified_labeled, result.rejected), (0, 2));
        assert!(result.hypotheses_from_verified_only.is_empty());
    }
}
