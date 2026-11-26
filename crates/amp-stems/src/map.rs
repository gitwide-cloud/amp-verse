//! Compact activity/timing data. This is NOT an audio separation mask.
//! It can schedule vocal-aware mixing or request live inference, never mute a voice by itself.
const MAGIC: &[u8; 8] = b"AMPACT01";
const HEADER: usize = 80;
pub const MAX_RECORD_BYTES: usize = 448;
pub const MAX_SEGMENTS: usize = (MAX_RECORD_BYTES - HEADER) / 12;
pub const DEFAULT_CATALOG_BYTES: u64 = 10_000_000;
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidIdentity,
    InvalidInterval,
    TooDetailed,
    InvalidRecord,
    BudgetExceeded,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub confidence: [u8; 4],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityMap {
    pub source_sha256: [u8; 32],
    pub model_sha256: [u8; 32],
    pub duration_ms: u32,
    pub segments: Vec<Segment>,
}
impl ActivityMap {
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        if self.source_sha256 == [0; 32] || self.model_sha256 == [0; 32] || self.duration_ms == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self.segments.len() > MAX_SEGMENTS {
            return Err(Error::TooDetailed);
        }
        let mut previous = 0;
        for s in &self.segments {
            if s.start_ms < previous || s.start_ms >= s.end_ms || s.end_ms > self.duration_ms {
                return Err(Error::InvalidInterval);
            }
            previous = s.end_ms;
        }
        let mut bytes = Vec::with_capacity(HEADER + self.segments.len() * 12);
        bytes.extend(MAGIC);
        bytes.extend(self.source_sha256);
        bytes.extend(self.model_sha256);
        bytes.extend(self.duration_ms.to_le_bytes());
        bytes.extend((self.segments.len() as u32).to_le_bytes());
        for s in &self.segments {
            bytes.extend(s.start_ms.to_le_bytes());
            bytes.extend(s.end_ms.to_le_bytes());
            bytes.extend(s.confidence);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER
            || bytes.len() > MAX_RECORD_BYTES
            || bytes.get(..8) != Some(MAGIC.as_slice())
        {
            return Err(Error::InvalidRecord);
        }
        let count = u32::from_le_bytes(bytes[76..80].try_into().unwrap()) as usize;
        if count > MAX_SEGMENTS || HEADER + count * 12 != bytes.len() {
            return Err(Error::InvalidRecord);
        }
        let map = Self {
            source_sha256: bytes[8..40].try_into().unwrap(),
            model_sha256: bytes[40..72].try_into().unwrap(),
            duration_ms: u32::from_le_bytes(bytes[72..76].try_into().unwrap()),
            segments: bytes[HEADER..]
                .as_chunks::<12>()
                .0
                .iter()
                .map(|s| Segment {
                    start_ms: u32::from_le_bytes(s[..4].try_into().unwrap()),
                    end_ms: u32::from_le_bytes(s[4..8].try_into().unwrap()),
                    confidence: s[8..12].try_into().unwrap(),
                })
                .collect(),
        };
        map.encode()?;
        Ok(map)
    }
}
/// Serialized catalog accounting, including planned index/length fields, not filesystem allocation.
/// No writer exists yet; any future writer must also account physical pages/journals atomically.
pub struct CatalogBudget {
    used: u64,
    limit: u64,
}
impl Default for CatalogBudget {
    fn default() -> Self {
        Self {
            used: 20,
            limit: DEFAULT_CATALOG_BYTES,
        }
    }
}
impl CatalogBudget {
    pub fn accounted_bytes(&self) -> u64 {
        self.used
    }
    pub fn reserve_record(&mut self, bytes: usize) -> Result<(), Error> {
        if !(HEADER..=MAX_RECORD_BYTES).contains(&bytes) {
            return Err(Error::InvalidRecord);
        }
        let next = self
            .used
            .checked_add(bytes as u64 + 48)
            .ok_or(Error::BudgetExceeded)?;
        if next > self.limit {
            return Err(Error::BudgetExceeded);
        }
        self.used = next;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn map(n: usize) -> ActivityMap {
        ActivityMap {
            source_sha256: [1; 32],
            model_sha256: [2; 32],
            duration_ms: 300000,
            segments: (0..n)
                .map(|i| Segment {
                    start_ms: i as u32 * 800,
                    end_ms: i as u32 * 800 + 500,
                    confidence: [200, 100, 80, 60],
                })
                .collect(),
        }
    }
    #[test]
    fn exact_roundtrip_and_corruption_rejected() {
        let m = map(20);
        let b = m.encode().unwrap();
        assert_eq!(b.len(), 320);
        assert_eq!(ActivityMap::decode(&b).unwrap(), m);
        assert!(ActivityMap::decode(&b[..b.len() - 1]).is_err());
        let mut bad = b.clone();
        bad[0] = 0;
        assert!(ActivityMap::decode(&bad).is_err());
    }
    #[test]
    fn twenty_thousand_largest_maps_stay_under10mb() {
        let b = map(MAX_SEGMENTS).encode().unwrap();
        let mut budget = CatalogBudget::default();
        for _ in 0..20000 {
            budget.reserve_record(b.len()).unwrap();
        }
        assert_eq!(budget.accounted_bytes(), 9_760_020);
        assert!(budget.accounted_bytes() < DEFAULT_CATALOG_BYTES);
    }
    #[test]
    fn budget_fails_closed_without_silent_growth() {
        let mut b = CatalogBudget::default();
        while b.reserve_record(MAX_RECORD_BYTES).is_ok() {}
        let before = b.accounted_bytes();
        assert_eq!(
            b.reserve_record(MAX_RECORD_BYTES),
            Err(Error::BudgetExceeded)
        );
        assert_eq!(b.accounted_bytes(), before);
    }
    #[test]
    fn excessive_detail_or_invalid_timeline_is_not_silently_discarded() {
        assert_eq!(map(MAX_SEGMENTS + 1).encode(), Err(Error::TooDetailed));
        let mut m = map(2);
        m.segments[1].start_ms = 100;
        assert_eq!(m.encode(), Err(Error::InvalidInterval));
    }
}
