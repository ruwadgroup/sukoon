//! Model registry.

use std::sync::RwLock;

use crate::{Error, Result};
#[cfg(feature = "onnx")]
use sha2::{Digest, Sha256};

/// A callback invoked while a model's weights download.
pub type DownloadObserver = Box<dyn Fn(&str, u64, Option<u64>) + Send + Sync>;

static DOWNLOAD_OBSERVER: RwLock<Option<DownloadObserver>> = RwLock::new(None);

/// Install a global download-progress observer.
pub fn set_download_observer(observer: DownloadObserver) {
    if let Ok(mut guard) = DOWNLOAD_OBSERVER.write() {
        *guard = Some(observer);
    }
}

/// Report download progress to the installed observer, if any.
#[cfg(feature = "onnx")]
fn notify_download(id: &str, downloaded: u64, total: Option<u64>) {
    if let Ok(guard) = DOWNLOAD_OBSERVER.read() {
        if let Some(observer) = guard.as_ref() {
            observer(id, downloaded, total);
        }
    }
}

/// How a model's weights are licensed — drives what's safe to redistribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum License {
    /// MIT — bundle freely.
    Mit,
    /// Apache-2.0 — bundle freely.
    Apache2,
    /// Permissive code license, but verify the upstream training-dataset terms before redistributing.
    PermissiveVerifyDataset,
    /// CC-BY-SA-4.0 — share-alike. Do NOT bundle in a closed binary; cloud-side only.
    CcBySa4,
    /// Community weights downloaded at runtime and never embedded in the binary.
    CommunityDownloadOnly,
}

impl License {
    /// Whether this license is safe to bundle inside a redistributed (possibly closed) binary.
    pub fn bundle_safe(self) -> bool {
        !matches!(self, License::CcBySa4 | License::CommunityDownloadOnly)
    }

    /// Human-readable SPDX-ish label.
    pub fn label(self) -> &'static str {
        match self {
            License::Mit => "MIT",
            License::Apache2 => "Apache-2.0",
            License::PermissiveVerifyDataset => "Permissive (verify dataset)",
            License::CcBySa4 => "CC-BY-SA-4.0",
            License::CommunityDownloadOnly => "Community (download-only, do not bundle)",
        }
    }
}

/// MDX-Net front-end parameters for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MdxParams {
    /// FFT size for the STFT front-end.
    pub n_fft: usize,
    /// Hop length between STFT frames.
    pub hop: usize,
    /// Frequency bins kept and fed to the model (`<= n_fft/2 + 1`).
    pub dim_f: usize,
    /// Time frames per inference chunk.
    pub dim_t: usize,
    /// Sample rate the model expects.
    pub sample_rate: u32,
    /// Whether the model's output stem is the vocals (`true`) or the instrumental (`false`).
    /// Either way Sukoon keeps the vocal/speech stem and derives the other by subtraction.
    pub output_is_vocals: bool,
}

/// A registry entry describing one downloadable model.
#[derive(Debug, Clone)]
pub struct Model {
    /// Stable engine id (matches the `Engine::id`).
    pub id: &'static str,
    /// Human-friendly display name.
    pub name: &'static str,
    /// Download URL for the weights (ONNX).
    pub url: &'static str,
    /// SHA-256 of the weights file, hex-encoded. Verified after download.
    pub sha256: &'static str,
    /// Approximate on-disk size in bytes (for UX/progress).
    pub size_bytes: u64,
    /// The weights license.
    pub license: License,
    /// MDX-Net front-end parameters, for MDX models. `None` for non-MDX engines.
    pub mdx: Option<MdxParams>,
}

/// The static registry. Update an entry to bump a model; note it in CHANGELOG/LICENSING.
const REGISTRY: &[Model] = &[
    Model {
        id: "deepfilternet",
        name: "DeepFilterNet 3 (Fast)",
        // Pinned DeepFilterNet ONNX tarball.
        url: "https://raw.githubusercontent.com/Rikorose/DeepFilterNet/d375b2d8/models/DeepFilterNet3_onnx.tar.gz",
        sha256: "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616",
        size_bytes: 7_983_136,
        license: License::Apache2,
        mdx: None,
    },
    Model {
        id: "mdx",
        name: "MDX-Net Kim Vocal 2 (HQ)",
        // UVR public model repo. Downloaded on first use; never bundled (see License).
        url: "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx",
        sha256: "ce74ef3b6a6024ce44211a07be9cf8bc6d87728cc852a68ab34eb8e58cde9c8b",
        size_bytes: 66_759_214,
        license: License::CommunityDownloadOnly,
        mdx: Some(MdxParams {
            n_fft: 6144,
            hop: 1024,
            dim_f: 3072,
            dim_t: 256,
            sample_rate: 44_100,
            output_is_vocals: true,
        }),
    },
    Model {
        id: "mdx-lite",
        name: "MDX-Net UVR 9482 (Fallback, low-RAM)",
        // Smaller UVR MDX vocal model using the same engine path.
        url: "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/UVR_MDXNET_9482.onnx",
        sha256: "f4f365207c56deb115bceedff3ad8fe98a751c745f9e370cecec6226b8b47184",
        size_bytes: 29_704_436,
        license: License::CommunityDownloadOnly,
        mdx: Some(MdxParams {
            n_fft: 6144,
            hop: 1024,
            dim_f: 2048,
            dim_t: 256,
            sample_rate: 44_100,
            output_is_vocals: true,
        }),
    },
];

impl Model {
    /// Look up a model by id.
    pub fn resolve(id: &str) -> Result<Model> {
        REGISTRY
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| Error::ModelUnavailable(format!("no registry entry for `{id}`")))
    }

    /// All known models.
    pub fn all() -> &'static [Model] {
        REGISTRY
    }

    /// Where the weights for this model are cached on disk.
    pub fn local_path(&self) -> std::path::PathBuf {
        models_dir().join(self.id).join("model.onnx")
    }

    /// Ensure the weights are present locally, downloading and verifying them on first use.
    #[cfg(feature = "onnx")]
    pub fn ensure_local(&self) -> Result<std::path::PathBuf> {
        let path = self.local_path();
        if path.exists() {
            return Ok(path);
        }
        if self.sha256.bytes().all(|b| b == b'0') {
            return Err(Error::ModelUnavailable(format!(
                "no real download URL/checksum registered for `{}`",
                self.id
            )));
        }
        let dir = path.parent().expect("model path has a parent");
        std::fs::create_dir_all(dir)?;

        tracing::debug!(
            model = self.id,
            url = self.url,
            size_mb = self.size_bytes / 1_000_000,
            "downloading weights (first use)"
        );

        let tmp = dir.join("model.onnx.partial");
        let resp = ureq::get(self.url)
            .call()
            .map_err(|e| Error::ModelUnavailable(format!("download `{}`: {e}", self.id)))?;
        let total = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
            .or(Some(self.size_bytes));
        let mut reader = resp.into_reader();
        let mut file = std::fs::File::create(&tmp)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut downloaded = 0u64;
        let mut last_tick = 0u64;
        notify_download(self.id, 0, total);
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            std::io::Write::write_all(&mut file, &buf[..n])?;
            downloaded += n as u64;
            if downloaded - last_tick >= 1 << 19 {
                last_tick = downloaded;
                notify_download(self.id, downloaded, total);
            }
        }
        drop(file);
        notify_download(self.id, downloaded, Some(downloaded));

        let got = hex::encode(hasher.finalize());
        if got != self.sha256 {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::ModelUnavailable(format!(
                "checksum mismatch for `{}`: expected {}, got {got}",
                self.id, self.sha256
            )));
        }
        std::fs::rename(&tmp, &path)?;
        tracing::debug!(model = self.id, path = %path.display(), "weights ready");
        Ok(path)
    }

    /// Ensure a **gzip-tar bundle** of weights is downloaded and extracted, returning the directory
    /// holding the extracted members. Used by multi-file models (DeepFilterNet ships three ONNX
    /// graphs in one tarball). The tarball's outer SHA-256 is verified before extraction; member
    /// paths are flattened (the upstream `tmp/export/` prefix is stripped).
    #[cfg(feature = "dfn")]
    pub fn ensure_extracted(&self, members: &[&str]) -> Result<std::path::PathBuf> {
        let dir = models_dir().join(self.id);
        let present = members.iter().all(|m| dir.join(m).exists());
        if present {
            return Ok(dir);
        }
        std::fs::create_dir_all(&dir)?;

        tracing::debug!(
            model = self.id,
            url = self.url,
            "downloading + extracting bundle"
        );
        let resp = ureq::get(self.url)
            .call()
            .map_err(|e| Error::ModelUnavailable(format!("download `{}`: {e}", self.id)))?;
        let total = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
            .or(Some(self.size_bytes));
        // Read the tarball into memory in chunks, reporting progress as we go.
        let mut reader = resp.into_reader();
        let mut bytes = Vec::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut last_tick = 0u64;
        notify_download(self.id, 0, total);
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if bytes.len() as u64 - last_tick >= 1 << 19 {
                last_tick = bytes.len() as u64;
                notify_download(self.id, bytes.len() as u64, total);
            }
        }
        notify_download(self.id, bytes.len() as u64, Some(bytes.len() as u64));

        let got = hex::encode(Sha256::digest(&bytes));
        if got != self.sha256 {
            return Err(Error::ModelUnavailable(format!(
                "checksum mismatch for `{}`: expected {}, got {got}",
                self.id, self.sha256
            )));
        }

        let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = tar::Archive::new(gz);
        for entry in archive
            .entries()
            .map_err(|e| Error::ModelUnavailable(format!("read tar `{}`: {e}", self.id)))?
        {
            let mut entry =
                entry.map_err(|e| Error::ModelUnavailable(format!("tar entry: {e}")))?;
            let path = entry
                .path()
                .map_err(|e| Error::ModelUnavailable(format!("tar path: {e}")))?
                .into_owned();
            // Match by file name so the `tmp/export/` prefix is irrelevant.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if members.contains(&name) {
                    let out = dir.join(name);
                    let tmp = dir.join(format!("{name}.partial"));
                    entry
                        .unpack(&tmp)
                        .map_err(|e| Error::ModelUnavailable(format!("extract {name}: {e}")))?;
                    std::fs::rename(&tmp, &out)?;
                }
            }
        }

        // Confirm everything we needed actually landed.
        for m in members {
            if !dir.join(m).exists() {
                return Err(Error::ModelUnavailable(format!(
                    "`{}` bundle missing member `{m}`",
                    self.id
                )));
            }
        }
        tracing::debug!(model = self.id, dir = %dir.display(), "bundle ready");
        Ok(dir)
    }
}

/// The directory where model weights are cached.
fn models_dir() -> std::path::PathBuf {
    std::env::var_os("SUKOON_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("sukoon").join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_bundle_safety() {
        // Permissive weights can be bundled; share-alike and unverified community weights cannot.
        assert!(License::Mit.bundle_safe());
        assert!(License::Apache2.bundle_safe());
        assert!(!License::CcBySa4.bundle_safe());
        assert!(!License::CommunityDownloadOnly.bundle_safe());
    }

    #[test]
    fn every_engine_has_a_registry_entry() {
        for id in ["deepfilternet", "mdx", "mdx-lite"] {
            assert!(
                Model::resolve(id).is_ok(),
                "missing registry entry for {id}"
            );
        }
    }

    #[test]
    fn mdx_model_is_download_only_with_params() {
        let mdx = Model::resolve("mdx").unwrap();
        assert!(mdx.mdx.is_some(), "mdx model must carry MDX params");
        assert!(
            !mdx.license.bundle_safe(),
            "mdx weights must not be bundled"
        );
        assert_eq!(mdx.license, License::CommunityDownloadOnly);
    }
}
