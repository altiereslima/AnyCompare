//! Diagnostic equality candidates for comparison with independently collected
//! BC5 reports. NONE of these policies is attributed to AnyCompare.
//! A candidate is merely a falsifiable hypothesis until actual BC5 output is
//! collected, verified, and reconciled field by field.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqualityHypothesis {
    /// Identical count, order and content.
    OrderedExact,
    /// Order-independent but duplicate multiplicity is significant.
    UnorderedMultiset,
    /// Order-independent with repeated identical values ignored.
    UniqueSet,
}

impl EqualityHypothesis {
    pub const ALL: [Self; 3] = [Self::OrderedExact, Self::UnorderedMultiset, Self::UniqueSet];

    pub fn predicts_equal(self, left: &[String], right: &[String]) -> bool {
        match self {
            Self::OrderedExact => left == right,
            Self::UnorderedMultiset => {
                let mut a = left.to_vec();
                let mut b = right.to_vec();
                a.sort();
                b.sort();
                a == b
            }
            Self::UniqueSet => {
                let a: BTreeSet<&str> = left.iter().map(String::as_str).collect();
                let b: BTreeSet<&str> = right.iter().map(String::as_str).collect();
                a == b
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OrderedExact => "ordered_exact",
            Self::UnorderedMultiset => "unordered_multiset",
            Self::UniqueSet => "unique_set",
        }
    }
}

/// An observable field pair. `observed_equal` MUST be taken from the original
/// BC5 report; do not fill it from our Rust comparator or a guessed rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedFieldCase {
    pub case_id: String,
    pub field_id: String,
    pub left: Vec<String>,
    pub right: Vec<String>,
    /// None until the relevant BC5 output has been collected.
    pub observed_equal: Option<bool>,
    /// SHA-256 of the original BC5 report, recorded for manual provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_report_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisScore {
    pub hypothesis: EqualityHypothesis,
    pub consistent_with_labeled_cases: bool,
    pub matches: usize,
    pub contradictions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationAudit {
    pub total_cases: usize,
    pub labeled_cases: usize,
    pub unlabeled_cases: usize,
    pub missing_provenance: Vec<String>,
    pub discriminatory_unlabeled_cases: Vec<String>,
    pub hypotheses: Vec<HypothesisScore>,
    /// Never claim 1:1 from a Rust hypothesis fit; an A/B run is required.
    pub bc5_parity_proven: bool,
}

/// Distinguishes policies only when real observations disagree with their
/// predictions. Reports possible but unresolved probes separately.
pub fn audit_equality_hypotheses(cases: &[ObservedFieldCase]) -> CalibrationAudit {
    let mut labeled = 0;
    let mut no_provenance = Vec::new();
    let mut discriminating = Vec::new();
    for item in cases {
        if item.observed_equal.is_some() {
            labeled += 1;
            if !is_sha256(item.original_report_sha256.as_deref()) {
                no_provenance.push(item.case_id.clone());
            }
        } else {
            let predictions: BTreeSet<bool> = EqualityHypothesis::ALL
                .iter()
                .map(|mode| mode.predicts_equal(&item.left, &item.right))
                .collect();
            if predictions.len() > 1 {
                discriminating.push(item.case_id.clone());
            }
        }
    }
    let hypotheses = EqualityHypothesis::ALL
        .iter()
        .map(|mode| {
            let mut matches = 0;
            let mut contradictions = Vec::new();
            for case in cases {
                if let Some(expected) = case.observed_equal {
                    if mode.predicts_equal(&case.left, &case.right) == expected {
                        matches += 1;
                    } else {
                        contradictions.push(case.case_id.clone());
                    }
                }
            }
            HypothesisScore {
                hypothesis: *mode,
                consistent_with_labeled_cases: contradictions.is_empty(),
                matches,
                contradictions,
            }
        })
        .collect();
    CalibrationAudit {
        total_cases: cases.len(),
        labeled_cases: labeled,
        unlabeled_cases: cases.len() - labeled,
        missing_provenance: no_provenance,
        discriminatory_unlabeled_cases: discriminating,
        hypotheses,
        bc5_parity_proven: false,
    }
}

fn is_sha256(s: Option<&str>) -> bool {
    match s {
        Some(v) => v.len() == 64 && v.bytes().all(|x| x.is_ascii_hexdigit()),
        None => false,
    }
}

/// Group evidence by field: two fields can have different equality behavior.
pub fn audit_by_field(cases: &[ObservedFieldCase]) -> BTreeMap<String, CalibrationAudit> {
    let mut groups: BTreeMap<String, Vec<ObservedFieldCase>> = BTreeMap::new();
    for c in cases {
        groups
            .entry(c.field_id.clone())
            .or_default()
            .push(c.clone());
    }
    groups
        .into_iter()
        .map(|(name, list)| (name, audit_equality_hypotheses(&list)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn case(id: &str, left: &[&str], right: &[&str], observed: Option<bool>) -> ObservedFieldCase {
        ObservedFieldCase {
            case_id: id.into(),
            field_id: "tag.artist".into(),
            left: left.iter().map(|s| (*s).to_owned()).collect(),
            right: right.iter().map(|s| (*s).to_owned()).collect(),
            observed_equal: observed,
            original_report_sha256: None,
        }
    }
    #[test]
    fn order_and_multiplicity_are_independently_testable() {
        let a = case("order", &["A", "B"], &["B", "A"], None);
        assert!(!EqualityHypothesis::OrderedExact.predicts_equal(&a.left, &a.right));
        assert!(EqualityHypothesis::UnorderedMultiset.predicts_equal(&a.left, &a.right));
        let b = case("repeat", &["A", "A"], &["A"], None);
        assert!(!EqualityHypothesis::UnorderedMultiset.predicts_equal(&b.left, &b.right));
        assert!(EqualityHypothesis::UniqueSet.predicts_equal(&b.left, &b.right));
    }
    #[test]
    fn unlabeled_inputs_never_prove_a_policy() {
        let report = audit_equality_hypotheses(&[case("order", &["A", "B"], &["B", "A"], None)]);
        assert_eq!(report.unlabeled_cases, 1);
        assert_eq!(report.discriminatory_unlabeled_cases, vec!["order"]);
        assert!(!report.bc5_parity_proven);
    }
    #[test]
    fn labeled_inputs_can_falsify_but_not_certify() {
        let report = audit_equality_hypotheses(&[case("repeat", &["A", "A"], &["A"], Some(false))]);
        assert_eq!(
            report
                .hypotheses
                .iter()
                .filter(|r| r.consistent_with_labeled_cases)
                .count(),
            2
        );
        assert_eq!(report.missing_provenance, vec!["repeat"]);
        assert!(!report.bc5_parity_proven);
    }
    #[test]
    fn scores_are_per_field_and_do_not_merge_tags() {
        let mut b = case("b", &["X"], &["Y"], Some(false));
        b.field_id = "tag.year".into();
        let grouped = audit_by_field(&[case("a", &["X"], &["X"], Some(true)), b]);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["tag.year"].labeled_cases, 1);
    }
}
