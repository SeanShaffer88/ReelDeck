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

pub mod hash;
pub mod log;
pub mod recipe;

pub use hash::{is_sha256_hex, sha256_digest, sha256_file, sha256_hex, Sha256};
pub use log::ExportLog;
pub use recipe::{
    CropRect, DeinterlaceEngine, DenoiseEngine, ExportPreset, FieldOrder, Operation, OutputRate,
    Recipe, RecipeError, RECIPE_SCHEMA_VERSION,
};
