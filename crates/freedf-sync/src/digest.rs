//! 콘텐츠 주소(CAS) 다이제스트 — `sha256:<hex64>` 형식의 강타입 래퍼.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

use crate::error::{Result, SyncError};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Digest(String);

impl Digest {
    /// 바이트에서 다이제스트 계산.
    pub fn from_bytes(data: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(data)))
    }

    /// 문자열 파싱·검증 (대소문자 허용, 내부적으로 소문자 정규화).
    pub fn parse(s: impl AsRef<str>) -> Result<Self> {
        let s = s.as_ref().trim();
        let Some(hex) = s.strip_prefix("sha256:") else {
            return Err(SyncError::Digest(format!("digest must be `sha256:<hex64>`: {s}")));
        };
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SyncError::Digest(format!("digest must be `sha256:<hex64>`: {s}")));
        }
        Ok(Self(format!("sha256:{}", hex.to_ascii_lowercase())))
    }

    /// `"sha256:…"` 전체 문자열.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `sha256:` 접두사 제외 hex.
    pub fn hex(&self) -> &str {
        &self.0[7..]
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Digest::parse(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_matches_known_sha256() {
        // sha256("") — 공식 테스트 벡터.
        let d = Digest::from_bytes(b"");
        assert_eq!(
            d.as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parse_validates_and_normalizes() {
        assert!(Digest::parse("sha256:").is_err());
        assert!(Digest::parse("abc").is_err());
        assert!(Digest::parse("sha256:xyz").is_err());
        assert!(Digest::parse(&format!("sha256:{}", "0".repeat(63))).is_err());
        let d = Digest::parse(&format!("sha256:{}", "A".repeat(64))).unwrap();
        assert_eq!(d.hex(), "a".repeat(64));
    }

    #[test]
    fn json_roundtrip() {
        let d = Digest::from_bytes(b"hello");
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(s, format!("\"{}\"", d.as_str()));
        let back: Digest = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }
}
