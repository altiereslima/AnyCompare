//! Independent Media Compare core. No GUI or proprietary BC5 runtime required.
//! The comparison is primarily about metadata, not waveform similarity.

pub mod calibration;
mod compare;
mod extract;
pub mod fidelity;
pub mod forensic;
mod format;
pub mod golden;
pub mod interpreted;
mod model;
mod report;
mod session;
pub mod source_guard;

pub use calibration::{
    audit_by_field, audit_equality_hypotheses, CalibrationAudit, EqualityHypothesis,
    HypothesisScore, ObservedFieldCase,
};
pub use compare::{compare_media, FieldComparison, FieldStatus, MediaComparison, MediaRules};
pub use extract::{extract_cover_art, inspect_media, InspectOptions};
pub use fidelity::{compare_candidate_policies, CandidateField, CandidateReport};
pub use forensic::{
    compare_raw_observations, observe_media_raw, RawDifference, RawDifferenceReport,
    RawMediaObservation, RawMediaRecord,
};
pub use format::{is_media_extension, MediaFormat, MediaProfile};
pub use golden::{
    audit_golden_evidence, EvidenceArtifact, EvidenceCase, EvidenceDecision, GoldenEvidenceAudit,
};
pub use interpreted::{
    interpret_raw_observation, observe_media_interpreted, InterpretedMediaObservation,
    InterpretedTag,
};
pub use model::{MediaError, MediaField, MediaInfo, MediaSource, MediaTag, PictureInfo};
pub use report::{
    render_html, render_html_with_options, render_text, render_text_with_options, to_json,
    ReportDisplay, ReportLayout, ReportOptions,
};
pub use session::{MediaSession, MediaViewFilter};
pub use source_guard::SourceFingerprint;
