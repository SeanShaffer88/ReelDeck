//! ReelDeck core logic.
//!
//! This crate is pure logic: no platform FFI, no clock or filesystem I/O
//! beyond streaming a file for its SHA-256. Everything here is
//! unit-testable on any platform. Analysis parsing lives in `rd-analyze`;
//! pipeline planning lives in `rd-pipeline`; presentation lives in
//! `rd-ui`.
//!
//! The design contract is `docs/ARCHITECTURE_AND_UX_PLAN.md` in the
//! repository root.

#![forbid(unsafe_code)]

pub mod batch;
pub mod hash;
pub mod log;
pub mod output;
pub mod recipe;
pub mod verify;

pub use batch::{
    attempt_output_path, Attempt, AttemptOutcome, BatchCounts, BatchEntry, BatchError,
    BatchManifest, BatchState, EntryState, EntryStatus, BATCH_SCHEMA_VERSION,
};
pub use hash::{is_sha256_hex, sha256_digest, sha256_file, sha256_hex, Sha256};
pub use log::{ExportLog, SourceRecord};
pub use output::{AudioCodec, Container, OutputError, OutputSettings, Rate, VideoCodec};
pub use recipe::{
    parse_time, CropRect, DeinterlaceEngine, DenoiseEngine, ExportPreset, FieldOrder, JoinPart,
    KeepRange, Metadata, Operation, OutputRate, Recipe, RecipeError, METADATA_MAX_CHARS,
    MIN_RECIPE_SCHEMA_VERSION, RECIPE_SCHEMA_VERSION,
};
pub use verify::{verify_export, verify_joined_export, Check, CheckStatus, VerificationReport};
