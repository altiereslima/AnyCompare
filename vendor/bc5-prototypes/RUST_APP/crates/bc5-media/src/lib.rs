//! Independent Media Compare core. No GUI or proprietary BC5 runtime required.
//! The comparison is primarily about metadata, not waveform similarity.

mod compare;
mod extract;
mod format;
pub mod fidelity;
pub mod forensic;
pub mod calibration;
pub mod golden;
pub mod source_guard;
pub mod interpreted;
mod model;
mod report;
pub mod reference;
mod session;

pub use fidelity::{compare_candidate_policies, CandidateReport, CandidateField};
pub use interpreted::{observe_media_interpreted, interpret_raw_observation, InterpretedMediaObservation, InterpretedTag};
pub use source_guard::SourceFingerprint;
pub use golden::{audit_golden_evidence, EvidenceCase, EvidenceArtifact, GoldenEvidenceAudit, EvidenceDecision};
pub use calibration::{EqualityHypothesis, ObservedFieldCase, HypothesisScore, CalibrationAudit, audit_equality_hypotheses, audit_by_field};
pub use forensic::{observe_media_raw, compare_raw_observations, RawMediaObservation, RawMediaRecord, RawDifference, RawDifferenceReport};
pub use compare::{compare_media, FieldComparison, FieldStatus, MediaComparison, MediaRules};
pub use extract::{extract_cover_art, inspect_media, InspectOptions};
pub use format::{is_media_extension, MediaFormat, MediaProfile};
pub use model::{MediaError, MediaField, MediaInfo, MediaSource, MediaTag, PictureInfo};
pub use report::{render_html, render_text, render_html_with_options, render_text_with_options, to_json, ReportOptions, ReportLayout, ReportDisplay};
pub use session::{MediaSession, MediaViewFilter};
