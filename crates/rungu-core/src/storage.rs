//! Storage backend for file attachments.
//!
//! Default driver: `fs` (filesystem).
//! S3-compatible driver (MinIO, Cloudflare R2, AWS S3) planned for future release.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

/// Supported image MIME types.
pub const ALLOWED_MIME_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];

/// Maximum upload size (10 MB).
pub const MAX_UPLOAD_SIZE: usize = 10 * 1024 * 1024;

/// Magic bytes for image type verification (don't trust Content-Type header).
fn verify_image_magic_bytes(data: &[u8]) -> Option<&'static str> {
    if data.len() < 4 {
        return None;
    }
    // PNG: 89 50 4E 47
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("image/png");
    }
    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    // WebP: 52 49 46 46 ?? ?? ?? ?? 57 45 42 50
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    // GIF: 47 49 46 38 (GIF8)
    if data.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    None
}

/// Verify uploaded data is actually a valid image.
/// Returns the detected MIME type or an error.
pub fn verify_image(data: &[u8], declared_mime: &str) -> Result<String> {
    if data.len() > MAX_UPLOAD_SIZE {
        bail!("File too large (max {} MB)", MAX_UPLOAD_SIZE / (1024 * 1024));
    }

    let detected = verify_image_magic_bytes(data)
        .context("Unrecognized image format — magic bytes do not match PNG, JPEG, WebP, or GIF")?;

    // Declared MIME must match detected (prevent spoofing)
    let declared_canonical = match declared_mime {
        "image/jpg" => "image/jpeg",
        other => other,
    };

    if detected != declared_canonical {
        bail!("MIME type mismatch: declared {declared_canonical}, detected {detected}");
    }

    Ok(detected.to_string())
}

/// Trait for storage backends.
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// Save a file and return its storage path/identifier.
    async fn save(&self, key: &str, data: Vec<u8>) -> Result<String>;

    /// Load a file by its storage path.
    async fn load(&self, path: &str) -> Result<Vec<u8>>;

    /// Delete a file by its storage path.
    async fn delete(&self, path: &str) -> Result<()>;
}

/// Filesystem storage backend (default).
pub struct FsStorage {
    base_dir: PathBuf,
}

impl FsStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self> {
        let base_dir = base_dir.into();
        // Create base directory if it doesn't exist
        std::fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create storage directory: {}", base_dir.display()))?;
        Ok(Self { base_dir })
    }

    fn resolve_path(&self, key: &str) -> Result<PathBuf> {
        // Prevent path traversal — reject keys with .. or absolute paths
        if key.contains("..") || key.starts_with('/') {
            bail!("Invalid storage key");
        }
        Ok(self.base_dir.join(key))
    }
}

#[async_trait::async_trait]
impl Storage for FsStorage {
    async fn save(&self, key: &str, data: Vec<u8>) -> Result<String> {
        let path = self.resolve_path(key)?;

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        tokio::fs::write(&path, &data).await.with_context(|| format!("Failed to write file: {}", path.display()))?;

        Ok(key.to_string())
    }

    async fn load(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.resolve_path(key)?;

        let mut file =
            tokio::fs::File::open(&path).await.with_context(|| format!("File not found: {}", path.display()))?;

        let mut data = Vec::new();
        file.read_to_end(&mut data).await.with_context(|| format!("Failed to read file: {}", path.display()))?;

        Ok(data)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.resolve_path(key)?;
        tokio::fs::remove_file(&path).await.with_context(|| format!("Failed to delete file: {}", path.display()))?;
        Ok(())
    }
}

/// Create the appropriate storage backend from environment variables.
pub fn create_storage() -> Result<Box<dyn Storage>> {
    let driver = std::env::var("STORAGE_DRIVER").unwrap_or_else(|_| "fs".to_string());

    match driver.as_str() {
        "fs" => {
            let dir = std::env::var("RUNGU_STORAGE_DIR").unwrap_or_else(|_| "./uploads".to_string());
            tracing::info!("Storage driver: fs, directory: {dir}");
            Ok(Box::new(FsStorage::new(dir)?))
        }
        "s3" => {
            // S3-compatible driver (MinIO, R2, AWS S3) — future implementation
            bail!("S3 storage driver not yet implemented. Use STORAGE_DRIVER=fs for now.")
        }
        other => bail!("Unknown STORAGE_DRIVER: {other}. Use 'fs' or 's3'."),
    }
}

/// Generate a unique storage key for an attachment.
pub fn storage_key(attachment_id: &str, mime: &str) -> String {
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    };
    // Distribute files across subdirectories to avoid huge flat dirs
    let prefix = &attachment_id[..2.min(attachment_id.len())];
    format!("{prefix}/{attachment_id}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let result = verify_image(&png_header, "image/png");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/png");
    }

    #[test]
    fn test_verify_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = verify_image(&jpeg_header, "image/jpeg");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/jpeg");
    }

    #[test]
    fn test_verify_jpg_alias() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = verify_image(&jpeg_header, "image/jpg");
        assert!(result.is_ok()); // "image/jpg" should be accepted as alias
    }

    #[test]
    fn test_verify_mime_mismatch() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let result = verify_image(&png_header, "image/jpeg");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_invalid_data() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = verify_image(&data, "image/png");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_too_large() {
        let data = vec![0x89u8, 0x50, 0x4E, 0x47]; // Valid PNG header
        let big_data = vec![0x89u8; MAX_UPLOAD_SIZE + 1];
        let result = verify_image(&big_data, "image/png");
        assert!(result.is_err());
        let _ = data; // suppress unused warning
    }

    #[test]
    fn test_storage_key() {
        let key = storage_key("abc123", "image/png");
        assert_eq!(key, "ab/abc123.png");

        let key = storage_key("xyz789", "image/jpeg");
        assert_eq!(key, "xy/xyz789.jpg");
    }

    #[tokio::test]
    async fn test_fs_storage_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = FsStorage::new(tmp.path()).unwrap();

        let data = b"fake image data".to_vec();
        let key = "ab/test_file.png";

        // Save
        let stored_path = storage.save(key, data.clone()).await.unwrap();
        assert_eq!(stored_path, key);

        // Load
        let loaded = storage.load(key).await.unwrap();
        assert_eq!(loaded, data);

        // Delete
        storage.delete(key).await.unwrap();

        // Load should fail
        assert!(storage.load(key).await.is_err());
    }

    #[test]
    fn test_fs_storage_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = FsStorage::new(tmp.path()).unwrap();

        let bad_path = storage.resolve_path("../../etc/passwd");
        assert!(bad_path.is_err());

        let bad_path2 = storage.resolve_path("/etc/passwd");
        assert!(bad_path2.is_err());
    }
}
