//! Output settings for conversions: container, codecs, picture size,
//! frame rate and bitrates (recipe schema 2, the `Custom` intent).
//!
//! Settings are validated against a deliberately conservative
//! compatibility table: a combination the container cannot carry, or a
//! bitrate on a codec that is not bitrate-controlled here, is refused
//! with the reason rather than adjusted. Saved settings files are how
//! output profiles for specific devices or platforms are kept.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Output container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    Avi,
}

impl Container {
    pub const ALL: [Container; 4] = [
        Container::Mp4,
        Container::Mov,
        Container::Mkv,
        Container::Avi,
    ];

    /// File extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "mkv",
            Container::Avi => "avi",
        }
    }

    /// The FFmpeg muxer name, passed explicitly with `-f`.
    pub fn muxer(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "matroska",
            Container::Avi => "avi",
        }
    }

    pub fn parse(text: &str) -> Option<Container> {
        Container::ALL
            .into_iter()
            .find(|c| c.extension() == text.to_ascii_lowercase())
    }

    /// The video codec used when none is chosen.
    pub fn default_video(self) -> VideoCodec {
        match self {
            Container::Avi => VideoCodec::Mpeg4,
            _ => VideoCodec::H264,
        }
    }

    /// The audio codec used when none is chosen. AVI defaults to PCM
    /// rather than MP3 because not every FFmpeg build has an MP3 encoder.
    pub fn default_audio(self) -> AudioCodec {
        match self {
            Container::Avi => AudioCodec::Pcm16,
            _ => AudioCodec::Aac,
        }
    }

    fn holds_video(self, codec: VideoCodec) -> bool {
        use VideoCodec::*;
        match self {
            Container::Mp4 => matches!(codec, H264 | H265 | Mpeg4),
            Container::Mov => matches!(codec, H264 | H265 | Mpeg4 | Prores),
            Container::Mkv => true,
            Container::Avi => matches!(codec, Mpeg4 | H264),
        }
    }

    fn holds_audio(self, codec: AudioCodec) -> bool {
        use AudioCodec::*;
        match self {
            Container::Mp4 => matches!(codec, Aac | Mp3 | None),
            Container::Mov => matches!(codec, Aac | Mp3 | Pcm16 | None),
            Container::Mkv => true,
            Container::Avi => matches!(codec, Mp3 | Pcm16 | None),
        }
    }
}

/// Output video codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    H265,
    Mpeg4,
    Prores,
    Ffv1,
}

impl VideoCodec {
    pub const ALL: [VideoCodec; 5] = [
        VideoCodec::H264,
        VideoCodec::H265,
        VideoCodec::Mpeg4,
        VideoCodec::Prores,
        VideoCodec::Ffv1,
    ];

    pub fn name(self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "h265",
            VideoCodec::Mpeg4 => "mpeg4",
            VideoCodec::Prores => "prores",
            VideoCodec::Ffv1 => "ffv1",
        }
    }

    pub fn parse(text: &str) -> Option<VideoCodec> {
        VideoCodec::ALL
            .into_iter()
            .find(|c| c.name() == text.to_ascii_lowercase())
    }

    /// The FFmpeg encoder this codec is written with.
    pub fn encoder(self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::Mpeg4 => "mpeg4",
            VideoCodec::Prores => "prores_ks",
            VideoCodec::Ffv1 => "ffv1",
        }
    }

    /// Whether a target bitrate applies; ProRes and FFV1 are profile- or
    /// lossless-coded instead.
    pub fn takes_bitrate(self) -> bool {
        matches!(
            self,
            VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Mpeg4
        )
    }
}

/// Output audio codec, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    Aac,
    Mp3,
    Pcm16,
    Flac,
    /// Write no audio.
    None,
}

impl AudioCodec {
    pub const ALL: [AudioCodec; 5] = [
        AudioCodec::Aac,
        AudioCodec::Mp3,
        AudioCodec::Pcm16,
        AudioCodec::Flac,
        AudioCodec::None,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Pcm16 => "pcm16",
            AudioCodec::Flac => "flac",
            AudioCodec::None => "none",
        }
    }

    pub fn parse(text: &str) -> Option<AudioCodec> {
        AudioCodec::ALL
            .into_iter()
            .find(|c| c.name() == text.to_ascii_lowercase())
    }

    /// The FFmpeg encoder, or `None` when no audio is written.
    pub fn encoder(self) -> Option<&'static str> {
        match self {
            AudioCodec::Aac => Some("aac"),
            AudioCodec::Mp3 => Some("libmp3lame"),
            AudioCodec::Pcm16 => Some("pcm_s16le"),
            AudioCodec::Flac => Some("flac"),
            AudioCodec::None => None,
        }
    }

    pub fn takes_bitrate(self) -> bool {
        matches!(self, AudioCodec::Aac | AudioCodec::Mp3)
    }
}

/// A frame rate as a fraction, e.g. 30000/1001.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}

impl Rate {
    /// `25`, `29.97` is not accepted (ambiguous); `30000/1001` is.
    pub fn parse(text: &str) -> Option<Rate> {
        let (num, den) = match text.trim().split_once('/') {
            Some((n, d)) => (n.trim().parse().ok()?, d.trim().parse().ok()?),
            None => (text.trim().parse().ok()?, 1),
        };
        Some(Rate { num, den })
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

impl std::fmt::Display for Rate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

/// Everything a conversion writes. Unset fields keep the source's value
/// (size, frame rate) or use the codec's quality-based default (bitrates).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSettings {
    pub container: Container,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    /// Output width and height in pixels; the picture is scaled to
    /// exactly this size.
    #[serde(default)]
    pub size: Option<(u32, u32)>,
    /// Output frame rate; frames are dropped or repeated to reach it
    /// (never interpolated).
    #[serde(default)]
    pub frame_rate: Option<Rate>,
    /// Target video bitrate in kbit/s; `None` uses the codec's quality
    /// default.
    #[serde(default)]
    pub video_bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub audio_bitrate_kbps: Option<u32>,
}

impl OutputSettings {
    /// Settings for a container with its default codecs.
    pub fn for_container(container: Container) -> Self {
        OutputSettings {
            container,
            video_codec: container.default_video(),
            audio_codec: container.default_audio(),
            size: None,
            frame_rate: None,
            video_bitrate_kbps: None,
            audio_bitrate_kbps: None,
        }
    }

    pub fn validate(&self) -> Result<(), OutputError> {
        let c = self.container;
        if !c.holds_video(self.video_codec) {
            return Err(OutputError::VideoNotInContainer {
                codec: self.video_codec.name(),
                container: c.extension(),
            });
        }
        if !c.holds_audio(self.audio_codec) {
            return Err(OutputError::AudioNotInContainer {
                codec: self.audio_codec.name(),
                container: c.extension(),
            });
        }
        if let Some((w, h)) = self.size {
            if w < 16 || h < 16 || w > 7680 || h > 4320 || w % 2 != 0 || h % 2 != 0 {
                return Err(OutputError::BadSize(w, h));
            }
        }
        if let Some(rate) = self.frame_rate {
            if rate.num == 0 || rate.den == 0 || rate.as_f64() > 300.0 {
                return Err(OutputError::BadFrameRate(rate.to_string()));
            }
        }
        if let Some(kbps) = self.video_bitrate_kbps {
            if !self.video_codec.takes_bitrate() {
                return Err(OutputError::BitrateNotApplicable(self.video_codec.name()));
            }
            if !(16..=500_000).contains(&kbps) {
                return Err(OutputError::BadBitrate("video", kbps));
            }
        }
        if let Some(kbps) = self.audio_bitrate_kbps {
            if !self.audio_codec.takes_bitrate() {
                return Err(OutputError::BitrateNotApplicable(self.audio_codec.name()));
            }
            if !(8..=1_024).contains(&kbps) {
                return Err(OutputError::BadBitrate("audio", kbps));
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// Why output settings are refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OutputError {
    #[error("the {container} container cannot carry {codec} video here; choose another codec or container")]
    VideoNotInContainer {
        codec: &'static str,
        container: &'static str,
    },
    #[error("the {container} container cannot carry {codec} audio here; choose another codec or container")]
    AudioNotInContainer {
        codec: &'static str,
        container: &'static str,
    },
    #[error("output size {0}x{1} is not usable: width and height must be even and between 16x16 and 7680x4320")]
    BadSize(u32, u32),
    #[error(
        "output frame rate {0} is not usable: it must be a positive fraction no higher than 300"
    )]
    BadFrameRate(String),
    #[error("{0} is not bitrate-controlled here; remove the bitrate")]
    BitrateNotApplicable(&'static str),
    #[error("{0} bitrate {1} kbit/s is outside the usable range")]
    BadBitrate(&'static str, u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_defaults_are_valid() {
        for c in Container::ALL {
            assert!(OutputSettings::for_container(c).validate().is_ok(), "{c:?}");
        }
        assert_eq!(Container::parse("MP4"), Some(Container::Mp4));
        assert_eq!(Container::parse("webm"), None);
        assert_eq!(Container::Mkv.muxer(), "matroska");
    }

    #[test]
    fn incompatible_combinations_are_refused() {
        let mut s = OutputSettings::for_container(Container::Mp4);
        s.video_codec = VideoCodec::Prores;
        assert_eq!(
            s.validate(),
            Err(OutputError::VideoNotInContainer {
                codec: "prores",
                container: "mp4"
            })
        );
        let mut s = OutputSettings::for_container(Container::Mp4);
        s.audio_codec = AudioCodec::Pcm16;
        assert!(matches!(
            s.validate(),
            Err(OutputError::AudioNotInContainer { .. })
        ));
        let mut s = OutputSettings::for_container(Container::Avi);
        s.video_codec = VideoCodec::H265;
        assert!(s.validate().is_err());
        let mut s = OutputSettings::for_container(Container::Mkv);
        s.video_codec = VideoCodec::Ffv1;
        s.audio_codec = AudioCodec::Flac;
        assert!(s.validate().is_ok(), "Matroska holds every codec here");
    }

    #[test]
    fn sizes_rates_and_bitrates_are_checked() {
        let base = OutputSettings::for_container(Container::Mp4);
        let with = |f: &dyn Fn(&mut OutputSettings)| {
            let mut s = base.clone();
            f(&mut s);
            s.validate()
        };
        assert!(with(&|s| s.size = Some((1280, 720))).is_ok());
        assert_eq!(
            with(&|s| s.size = Some((1281, 720))),
            Err(OutputError::BadSize(1281, 720))
        );
        assert!(with(&|s| s.frame_rate = Rate::parse("30000/1001")).is_ok());
        assert!(with(&|s| s.frame_rate = Some(Rate { num: 25, den: 0 })).is_err());
        assert!(with(&|s| s.frame_rate = Some(Rate { num: 1000, den: 1 })).is_err());
        assert!(with(&|s| s.video_bitrate_kbps = Some(4000)).is_ok());
        assert!(with(&|s| s.video_bitrate_kbps = Some(1)).is_err());
        assert!(with(&|s| s.audio_bitrate_kbps = Some(192)).is_ok());
        assert_eq!(
            with(&|s| {
                s.audio_codec = AudioCodec::None;
                s.audio_bitrate_kbps = Some(192);
            }),
            Err(OutputError::BitrateNotApplicable("none"))
        );
        let mut prores = OutputSettings::for_container(Container::Mov);
        prores.video_codec = VideoCodec::Prores;
        prores.video_bitrate_kbps = Some(50_000);
        assert_eq!(
            prores.validate(),
            Err(OutputError::BitrateNotApplicable("prores"))
        );
    }

    #[test]
    fn rates_parse_and_display() {
        assert_eq!(Rate::parse("25"), Some(Rate { num: 25, den: 1 }));
        assert_eq!(Rate::parse("30000/1001").unwrap().to_string(), "30000/1001");
        assert_eq!(Rate::parse("29.97"), None);
        assert_eq!(Rate::parse("x"), None);
    }

    #[test]
    fn settings_round_trip_as_a_profile_file() {
        let mut s = OutputSettings::for_container(Container::Mov);
        s.size = Some((1920, 1080));
        s.frame_rate = Rate::parse("25");
        let back = OutputSettings::from_json(&s.to_json().unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
