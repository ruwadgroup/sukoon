//! Content-hash keyed cache of cleaned audio stems.
//!
//! Processing the same content twice is wasteful, so the first run is cached and every replay is
//! instant. Keys are derived from the input bytes + engine + mode, so changing any of them is a
//! cache miss. Cache is **local-only by default** for privacy; retention is the shell's call.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// A cache key: a hex SHA-256 over input content and the parameters that affect output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey(String);

impl CacheKey {
    /// Build a key from input file bytes plus the engine id and mode string.
    pub fn from_file(input: &Path, engine_id: &str, mode: &str) -> std::io::Result<Self> {
        let bytes = std::fs::read(input)?;
        Ok(Self::from_bytes(&bytes, engine_id, mode))
    }

    /// Build a key from raw content bytes plus parameters.
    pub fn from_bytes(content: &[u8], engine_id: &str, mode: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hasher.update([0]);
        hasher.update(engine_id.as_bytes());
        hasher.update([0]);
        hasher.update(mode.as_bytes());
        CacheKey(hex::encode(hasher.finalize()))
    }

    /// The hex digest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A simple on-disk cache directory.
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// Open (creating if needed) a cache rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// The default per-user cache location, overridable via `SUKOON_CACHE_DIR`.
    pub fn default_location() -> std::io::Result<Self> {
        let root = std::env::var_os("SUKOON_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("sukoon").join("cache"));
        Self::open(root)
    }

    /// Path a cached stem would live at for the given key.
    pub fn path_for(&self, key: &CacheKey) -> PathBuf {
        self.root.join(format!("{}.wav", key.as_str()))
    }

    /// Whether a cleaned stem is cached for this key.
    pub fn contains(&self, key: &CacheKey) -> bool {
        self.path_for(key).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_engine_and_mode() {
        let a = CacheKey::from_bytes(b"audio", "mdx", "remove-all");
        let b = CacheKey::from_bytes(b"audio", "deepfilternet", "remove-all");
        let c = CacheKey::from_bytes(b"audio", "mdx", "keep-vocals");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, CacheKey::from_bytes(b"audio", "mdx", "remove-all"));
    }
}
