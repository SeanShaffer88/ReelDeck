//! Safe Rust wrapper over the MediaKit DLL.
//!
//! MediaKit probes media *headers* — container, duration, bitrate,
//! and per-stream codec / dimensions / sample-rate / channel /
//! language facts — and reports them as JSON over a C ABI
//! (`me_probe`). This crate is the one place in the ReelDeck
//! workspace where `unsafe` is allowed: the FFI declarations in
//! [`ffi`] match MediaKit's `include/mediakit.h` (v0.1.0) exactly,
//! and every unsafe block is commented with the invariants that make
//! it sound. Everything above the FFI layer is safe, typed, and
//! unit-tested without the DLL.
//!
//! On non-Windows targets nothing is linked and [`probe`] /
//! [`probe_bytes`] report [`MediaKitError::Unavailable`] — an honest
//! stub, never fabricated probe data — so the workspace builds and
//! tests pass on any platform. Windows runtime behaviour against
//! the real DLL is verified only by running on Windows.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error codes (ME_ERR_* in mediakit.h)
// ---------------------------------------------------------------------------

/// A MediaKit error code, as reported by `me_last_error_code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKitErrorCode {
    Ok,
    InvalidArg,
    NotSupported,
    NotFound,
    AccessDenied,
    Platform,
    Internal,
    Parse,
    /// A code this wrapper does not know (newer DLL than wrapper).
    Unknown(i32),
}

impl MediaKitErrorCode {
    /// Map a raw `me_last_error_code()` value onto the typed code.
    /// `0` is `ME_OK`; `1..=7` are the `ME_ERR_*` constants in the
    /// header, in order; anything else is preserved, not guessed at.
    pub fn from_raw(code: i32) -> Self {
        match code {
            0 => MediaKitErrorCode::Ok,
            1 => MediaKitErrorCode::InvalidArg,
            2 => MediaKitErrorCode::NotSupported,
            3 => MediaKitErrorCode::NotFound,
            4 => MediaKitErrorCode::AccessDenied,
            5 => MediaKitErrorCode::Platform,
            6 => MediaKitErrorCode::Internal,
            7 => MediaKitErrorCode::Parse,
            other => MediaKitErrorCode::Unknown(other),
        }
    }

    /// The raw header value for this code.
    pub fn raw(self) -> i32 {
        match self {
            MediaKitErrorCode::Ok => 0,
            MediaKitErrorCode::InvalidArg => 1,
            MediaKitErrorCode::NotSupported => 2,
            MediaKitErrorCode::NotFound => 3,
            MediaKitErrorCode::AccessDenied => 4,
            MediaKitErrorCode::Platform => 5,
            MediaKitErrorCode::Internal => 6,
            MediaKitErrorCode::Parse => 7,
            MediaKitErrorCode::Unknown(code) => code,
        }
    }
}

#[derive(Debug, Error)]
pub enum MediaKitError {
    /// MediaKit cannot run here at all (non-Windows target). Callers
    /// are expected to fall back to another probe source.
    #[error("MediaKit is unavailable on this platform: {0}")]
    Unavailable(String),
    /// The DLL ran and reported a failure: its code and its own
    /// thread-local message, copied out before any further call.
    #[error("MediaKit failed ({code:?}, raw {}): {message}", code.raw())]
    Failed {
        code: MediaKitErrorCode,
        message: String,
    },
    /// The DLL returned text that is not the documented JSON shape.
    #[error("could not parse MediaKit probe JSON: {0}")]
    Parse(String),
}

// ---------------------------------------------------------------------------
// Probe model (MediaKit's documented JSON shape)
// ---------------------------------------------------------------------------

/// One stream in MediaKit's probe result. A field is `None` when the
/// file does not carry that value; MediaKit never guesses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaKitStream {
    /// `video`, `audio`, `subtitle`, or `other`.
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub channels: Option<u32>,
    #[serde(default)]
    pub language: Option<String>,
}

/// Container-level facts in MediaKit's probe result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaKitProbe {
    pub container: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// True only for MP3 files without a Xing/Info header, where the
    /// duration is derived from file size at the first frame's
    /// bitrate. Everything else is an exact value or `None`.
    #[serde(default)]
    pub duration_is_estimate: bool,
    #[serde(default)]
    pub bitrate_bps: Option<u64>,
    #[serde(default)]
    pub streams: Vec<MediaKitStream>,
}

/// Parse probe JSON in MediaKit's documented shape. Pure and
/// platform-independent; this is what the mapping unit tests use.
pub fn parse_probe_json(text: &str) -> Result<MediaKitProbe, MediaKitError> {
    serde_json::from_str(text).map_err(|e| MediaKitError::Parse(e.to_string()))
}

// ---------------------------------------------------------------------------
// FFI declarations — Windows only, matching include/mediakit.h exactly
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub mod ffi {
    use std::os::raw::{c_char, c_int, c_uchar};

    // The declarations below mirror mediakit.h (MediaKit v0.1.0)
    // one for one, including argument and return types. Strings are
    // UTF-8, NUL-terminated. `char*` results of me_probe /
    // me_probe_bytes are caller-owned and must be freed exactly once
    // with me_free_string; me_version / me_last_error results are
    // library-owned and must never be freed.
    extern "C" {
        pub fn me_version() -> *const c_char;
        pub fn me_last_error() -> *const c_char;
        pub fn me_last_error_code() -> c_int;
        pub fn me_free_string(p: *mut c_char);
        pub fn me_probe(path: *const c_char) -> *mut c_char;
        pub fn me_probe_bytes(data: *const c_uchar, len: usize) -> *mut c_char;
    }
}

// ---------------------------------------------------------------------------
// Safe wrapper
// ---------------------------------------------------------------------------

/// True when the MediaKit probe is compiled into this binary
/// (Windows). On any other platform it is false and [`probe`]
/// reports [`MediaKitError::Unavailable`]. Note this says nothing
/// about whether `media_kit.dll` is present beside the executable at
/// run time — Windows refuses to start a program whose import DLL
/// is missing, so a running Windows program has the DLL by
/// construction.
pub fn is_available() -> bool {
    cfg!(windows)
}

/// The MediaKit version string (e.g. `"0.1.0"`) on Windows, or
/// `None` on other platforms / if the DLL returns no string.
pub fn version() -> Option<String> {
    #[cfg(windows)]
    {
        // SAFETY: me_version returns a pointer to a static,
        // NUL-terminated, library-owned string (header contract); we
        // only read it, copy it, and never free it.
        unsafe {
            let ptr = ffi::me_version();
            if ptr.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
            }
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Read the calling thread's last MediaKit error as a typed
/// [`MediaKitError::Failed`], copying code and message out before
/// any other MediaKit call can overwrite them (thread-local state
/// per the header contract).
#[cfg(windows)]
fn last_error() -> MediaKitError {
    // SAFETY: me_last_error_code takes no arguments and returns a
    // plain int; me_last_error returns a thread-local, NUL-terminated
    // string that stays valid until the next MediaKit call on this
    // thread. We copy both immediately and never free the message
    // pointer (it is library-owned).
    unsafe {
        let code = MediaKitErrorCode::from_raw(ffi::me_last_error_code());
        let ptr = ffi::me_last_error();
        let message = if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        };
        MediaKitError::Failed { code, message }
    }
}

/// Turn a MediaKit-owned result pointer into an owned Rust string
/// and a parsed probe, freeing the pointer exactly once.
#[cfg(windows)]
fn take_probe_result(ptr: *mut std::os::raw::c_char) -> Result<MediaKitProbe, MediaKitError> {
    if ptr.is_null() {
        return Err(last_error());
    }
    // SAFETY: ptr is a non-null, caller-owned string returned by
    // me_probe / me_probe_bytes. We copy its bytes to an owned
    // String first, then return the pointer to the library with
    // me_free_string exactly once — the only free on every path
    // below, including parse failure.
    let text = unsafe {
        let copied = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
        ffi::me_free_string(ptr);
        copied
    };
    parse_probe_json(&text)
}

/// Probe a media file on disk with MediaKit.
///
/// On Windows this calls `me_probe`. On every other platform it
/// returns [`MediaKitError::Unavailable`] without touching the file:
/// header facts are never fabricated.
pub fn probe(path: &Path) -> Result<MediaKitProbe, MediaKitError> {
    #[cfg(windows)]
    {
        let Some(path_str) = path.to_str() else {
            return Err(MediaKitError::Failed {
                code: MediaKitErrorCode::InvalidArg,
                message: "path is not valid UTF-8".to_string(),
            });
        };
        let Ok(c_path) = std::ffi::CString::new(path_str) else {
            return Err(MediaKitError::Failed {
                code: MediaKitErrorCode::InvalidArg,
                message: "path contains an interior NUL byte".to_string(),
            });
        };
        // SAFETY: c_path is a valid NUL-terminated UTF-8 string that
        // outlives the call. The returned pointer is caller-owned
        // and handled (copied + freed, or turned into the thread's
        // last error) by take_probe_result.
        let ptr = unsafe { ffi::me_probe(c_path.as_ptr()) };
        take_probe_result(ptr)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(MediaKitError::Unavailable(
            "the MediaKit DLL is a Windows library".to_string(),
        ))
    }
}

/// Probe media bytes already in memory with MediaKit (`me_probe_bytes`).
///
/// An empty slice is `ME_ERR_INVALID_ARG` on every platform, per
/// the header contract, and is checked before the platform stub so
/// the behaviour is identical everywhere.
pub fn probe_bytes(data: &[u8]) -> Result<MediaKitProbe, MediaKitError> {
    if data.is_empty() {
        return Err(MediaKitError::Failed {
            code: MediaKitErrorCode::InvalidArg,
            message: "a NULL data pointer or a len of 0 is ME_ERR_INVALID_ARG".to_string(),
        });
    }
    #[cfg(windows)]
    {
        // SAFETY: data.as_ptr() points to data.len() readable bytes
        // and the slice outlives the call. The returned pointer is
        // caller-owned and handled by take_probe_result.
        let ptr = unsafe { ffi::me_probe_bytes(data.as_ptr(), data.len()) };
        take_probe_result(ptr)
    }
    #[cfg(not(windows))]
    {
        Err(MediaKitError::Unavailable(
            "the MediaKit DLL is a Windows library".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MediaKit's documented shape: an MP4 with one video and one
    /// audio stream, every field populated.
    const FULL_FIXTURE: &str = r#"{
      "container": "mp4",
      "duration_ms": 123456,
      "duration_is_estimate": false,
      "bitrate_bps": 1234567,
      "streams": [
        {"kind": "video", "codec": "avc1", "width": 1920, "height": 1080,
         "sample_rate": null, "channels": null, "language": null},
        {"kind": "audio", "codec": "mp4a", "width": null, "height": null,
         "sample_rate": 48000, "channels": 2, "language": "eng"}
      ]
    }"#;

    /// Nulls everywhere the file carries no value, and an estimated
    /// MP3 duration (the one case `duration_is_estimate` may be true).
    const NULLS_AND_ESTIMATE_FIXTURE: &str = r#"{
      "container": "mp3",
      "duration_ms": 187000,
      "duration_is_estimate": true,
      "bitrate_bps": null,
      "streams": [
        {"kind": "audio", "codec": null, "width": null, "height": null,
         "sample_rate": null, "channels": null, "language": null}
      ]
    }"#;

    #[test]
    fn parses_full_fixture() {
        let probe = parse_probe_json(FULL_FIXTURE).expect("parse");
        assert_eq!(probe.container, "mp4");
        assert_eq!(probe.duration_ms, Some(123_456));
        assert!(!probe.duration_is_estimate);
        assert_eq!(probe.bitrate_bps, Some(1_234_567));
        assert_eq!(probe.streams.len(), 2);
        let video = &probe.streams[0];
        assert_eq!(video.kind, "video");
        assert_eq!(video.codec.as_deref(), Some("avc1"));
        assert_eq!(video.width, Some(1920));
        assert_eq!(video.height, Some(1080));
        let audio = &probe.streams[1];
        assert_eq!(audio.sample_rate, Some(48_000));
        assert_eq!(audio.channels, Some(2));
        assert_eq!(audio.language.as_deref(), Some("eng"));
    }

    #[test]
    fn parses_null_fields_and_estimated_duration() {
        let probe = parse_probe_json(NULLS_AND_ESTIMATE_FIXTURE).expect("parse");
        assert_eq!(probe.container, "mp3");
        assert_eq!(probe.duration_ms, Some(187_000));
        assert!(probe.duration_is_estimate);
        assert_eq!(probe.bitrate_bps, None);
        let audio = &probe.streams[0];
        assert_eq!(audio.kind, "audio");
        assert_eq!(audio.codec, None);
        assert_eq!(audio.sample_rate, None);
        assert_eq!(audio.language, None);
    }

    #[test]
    fn bad_json_is_a_parse_error() {
        assert!(matches!(
            parse_probe_json("{not json"),
            Err(MediaKitError::Parse(_))
        ));
        // Missing the one required field (container) is also a parse
        // error rather than a defaulted value.
        assert!(matches!(
            parse_probe_json(r#"{"duration_ms": 1}"#),
            Err(MediaKitError::Parse(_))
        ));
    }

    #[test]
    fn error_code_mapping_matches_header() {
        // Every ME_ERR_* constant from mediakit.h, in both directions.
        let cases = [
            (0, MediaKitErrorCode::Ok),
            (1, MediaKitErrorCode::InvalidArg),
            (2, MediaKitErrorCode::NotSupported),
            (3, MediaKitErrorCode::NotFound),
            (4, MediaKitErrorCode::AccessDenied),
            (5, MediaKitErrorCode::Platform),
            (6, MediaKitErrorCode::Internal),
            (7, MediaKitErrorCode::Parse),
        ];
        for (raw, typed) in cases {
            assert_eq!(MediaKitErrorCode::from_raw(raw), typed);
            assert_eq!(typed.raw(), raw);
        }
        // Unknown codes are preserved, never folded into a guess.
        assert_eq!(
            MediaKitErrorCode::from_raw(99),
            MediaKitErrorCode::Unknown(99)
        );
        assert_eq!(MediaKitErrorCode::Unknown(99).raw(), 99);
        assert_eq!(
            MediaKitErrorCode::from_raw(-1),
            MediaKitErrorCode::Unknown(-1)
        );
    }

    #[test]
    fn probe_bytes_rejects_empty_input_on_every_platform() {
        match probe_bytes(&[]) {
            Err(MediaKitError::Failed { code, .. }) => {
                assert_eq!(code, MediaKitErrorCode::InvalidArg);
            }
            other => panic!("empty input must be InvalidArg, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn stub_reports_unavailable_off_windows() {
        assert!(!is_available());
        assert_eq!(version(), None);
        assert!(matches!(
            probe(Path::new("anything.mp4")),
            Err(MediaKitError::Unavailable(_))
        ));
        assert!(matches!(
            probe_bytes(b"not a real file"),
            Err(MediaKitError::Unavailable(_))
        ));
    }
}
