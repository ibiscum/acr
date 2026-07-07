/// Stream format details representing audio format information
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamDetails {
    /// Sample rate in Hz (e.g., 44100, 48000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,

    /// Bits per sample (e.g., 16, 24, 32)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<u8>,

    /// Number of audio channels (e.g., 1 for mono, 2 for stereo)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,

    /// Type of sample encoding (e.g., "pcm", "dsd", "mqa")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_type: Option<String>,

    /// Indicates if the stream is lossless or lossy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossless: Option<bool>,
}

impl StreamDetails {
    /// Create a new empty StreamDetails
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate bits per second (bitrate) if sample information is available
    /// Returns None if any required information is missing
    pub fn bitrate(&self) -> Option<u64> {
        if let (Some(rate), Some(bits), Some(channels)) =
            (self.sample_rate, self.bits_per_sample, self.channels) {
            Some(u64::from(rate) * u64::from(bits) * u64::from(channels))
        } else {
            None
        }
    }

    /// Create a human-readable description of the stream format
    pub fn format_description(&self) -> String {
        let mut parts = Vec::new();

        // Add sample rate if available
        if let Some(rate) = self.sample_rate {
            if rate >= 1000 {
                let whole = rate / 1000;
                let frac = rate % 1000;
                let formatted = if frac == 0 {
                    format!("{} kHz", whole)
                } else if frac % 100 == 0 {
                    format!("{}.{} kHz", whole, frac / 100)
                } else if frac % 10 == 0 {
                    format!("{}.{:02} kHz", whole, frac / 10)
                } else {
                    format!("{}.{:03} kHz", whole, frac)
                };
                parts.push(formatted);
            } else {
                parts.push(format!("{} Hz", rate));
            }
        }

        // Add bit depth and sample type if available
        if let Some(bits) = self.bits_per_sample {
            if let Some(sample_type) = &self.sample_type {
                if sample_type.eq_ignore_ascii_case("pcm") {
                    parts.push(format!("{}-bit", bits));
                } else {
                    parts.push(format!("{}-bit {}", bits, sample_type.to_uppercase()));
                }
            } else {
                parts.push(format!("{}-bit", bits));
            }
        } else if let Some(sample_type) = &self.sample_type {
            parts.push(sample_type.to_uppercase());
        }

        // Add channel information
        if let Some(channels) = self.channels {
            match channels {
                1 => parts.push("Mono".to_string()),
                2 => parts.push("Stereo".to_string()),
                _ => parts.push(format!("{} channels", channels)),
            }
        }

        // Add lossless indicator
        if let Some(lossless) = self.lossless {
            parts.push(if lossless { "Lossless".to_string() } else { "Lossy".to_string() });
        }

        // Join all parts with spaces
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::StreamDetails;

    fn make(sample_rate: u32, bits: u8, channels: u8) -> StreamDetails {
        StreamDetails {
            sample_rate: Some(sample_rate),
            bits_per_sample: Some(bits),
            channels: Some(channels),
            ..StreamDetails::default()
        }
    }

    // --- bitrate ---

    #[test]
    fn bitrate_returns_none_when_fields_missing() {
        assert_eq!(StreamDetails::default().bitrate(), None);
        assert_eq!(StreamDetails { sample_rate: Some(44100), ..Default::default() }.bitrate(), None);
    }

    #[test]
    fn bitrate_computes_pcm_uncompressed_rate() {
        // 44100 * 16 * 2 = 1_411_200
        assert_eq!(make(44100, 16, 2).bitrate(), Some(1_411_200));
        // 192000 * 24 * 2 = 9_216_000
        assert_eq!(make(192000, 24, 2).bitrate(), Some(9_216_000));
    }

    // --- format_description: sample rate ---

    #[test]
    fn format_description_whole_khz() {
        let s = StreamDetails { sample_rate: Some(48000), ..Default::default() };
        assert!(s.format_description().starts_with("48 kHz"), "{}", s.format_description());
    }

    #[test]
    fn format_description_one_decimal_khz() {
        let s = StreamDetails { sample_rate: Some(44100), ..Default::default() };
        assert!(s.format_description().starts_with("44.1 kHz"), "{}", s.format_description());
    }

    #[test]
    fn format_description_two_decimal_khz() {
        // 44110 Hz → 44.11 kHz
        let s = StreamDetails { sample_rate: Some(44110), ..Default::default() };
        assert!(s.format_description().starts_with("44.11 kHz"), "{}", s.format_description());
    }

    #[test]
    fn format_description_three_decimal_khz() {
        // 44101 Hz → 44.101 kHz (no information lost)
        let s = StreamDetails { sample_rate: Some(44101), ..Default::default() };
        assert!(s.format_description().starts_with("44.101 kHz"), "{}", s.format_description());
    }

    #[test]
    fn format_description_sub_khz_hz() {
        let s = StreamDetails { sample_rate: Some(800), ..Default::default() };
        assert!(s.format_description().starts_with("800 Hz"), "{}", s.format_description());
    }

    // --- format_description: bits and sample_type ---

    #[test]
    fn format_description_pcm_suppresses_type_label() {
        let s = StreamDetails {
            sample_rate: Some(44100),
            bits_per_sample: Some(16),
            sample_type: Some("pcm".to_string()),
            ..Default::default()
        };
        let desc = s.format_description();
        assert!(desc.contains("16-bit"), "{}", desc);
        assert!(!desc.to_lowercase().contains("pcm"), "{}", desc);
    }

    #[test]
    fn format_description_non_pcm_shows_type_label() {
        let s = StreamDetails {
            bits_per_sample: Some(1),
            sample_type: Some("dsd".to_string()),
            ..Default::default()
        };
        let desc = s.format_description();
        assert!(desc.contains("1-bit DSD"), "{}", desc);
    }

    #[test]
    fn format_description_sample_type_only_uppercased() {
        let s = StreamDetails {
            sample_type: Some("mqa".to_string()),
            ..Default::default()
        };
        assert_eq!(s.format_description(), "MQA");
    }

    // --- format_description: channels ---

    #[test]
    fn format_description_channel_labels() {
        let mut s = StreamDetails { channels: Some(1), ..Default::default() };
        assert_eq!(s.format_description(), "Mono");
        s.channels = Some(2);
        assert_eq!(s.format_description(), "Stereo");
        s.channels = Some(6);
        assert_eq!(s.format_description(), "6 channels");
    }

    // --- format_description: empty ---

    #[test]
    fn format_description_empty_returns_empty_string() {
        assert_eq!(StreamDetails::default().format_description(), "");
    }

    // --- format_description: lossless indicator ---

    #[test]
    fn format_description_lossless_flag() {
        let lossless = StreamDetails { lossless: Some(true), ..Default::default() };
        assert_eq!(lossless.format_description(), "Lossless");
        let lossy = StreamDetails { lossless: Some(false), ..Default::default() };
        assert_eq!(lossy.format_description(), "Lossy");
    }
}
