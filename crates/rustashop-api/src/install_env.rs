//! Generate secrets and upsert keys into `.env`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::admin_auth::ADMIN_TOKEN_ENV;
use crate::admin_prefix::{ADMIN_API_PREFIX_ENV, AdminApiPrefix, DEFAULT_ADMIN_API_PREFIX};
use crate::install_fs::shop_root;

/// Env path override (defaults to `{root}/.env`).
pub const ENV_FILE_ENV: &str = "RUSTASHOP_ENV_FILE";

/// Serializes tests that mutate process env for install root / env file.
#[cfg(test)]
pub static INSTALL_PROCESS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Result of a successful install write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallWriteResult {
    /// Opaque admin / API segment.
    pub admin_prefix: String,
    /// Bearer token written for operator API.
    pub admin_token: String,
    /// Absolute path to the env file updated.
    pub env_path: PathBuf,
}

/// Errors from install env generation / write.
#[derive(Debug, thiserror::Error)]
pub enum InstallEnvError {
    /// Overwrite refused without wipe acknowledgement.
    #[error("admin prefix already set; pass wipe confirmation to overwrite")]
    WipeRequired,
    /// Invalid optional admin folder segment.
    #[error("invalid admin folder: {0}")]
    InvalidPrefix(String),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Options for [`run_install_write`].
#[derive(Debug, Clone, Default)]
pub struct InstallWriteOptions {
    /// Optional explicit segment; otherwise random opaque.
    pub admin_folder: Option<String>,
    /// Required when `.env` already has a non-default admin prefix.
    pub wipe_confirmed: bool,
}

/// Path to the env file used by install.
#[must_use]
pub fn env_file_path(root: &Path) -> PathBuf {
    std::env::var_os(ENV_FILE_ENV).map_or_else(|| root.join(".env"), PathBuf::from)
}

/// True when an existing env already configures a non-default admin prefix.
#[must_use]
pub fn existing_prefix_needs_wipe(root: &Path) -> bool {
    let path = env_file_path(root);
    let Ok(contents) = fs::read_to_string(&path) else {
        return false;
    };
    matches!(
        read_env_value(&contents, ADMIN_API_PREFIX_ENV),
        Some(value) if !value.is_empty() && value != DEFAULT_ADMIN_API_PREFIX
    )
}

/// Generate segment + token and upsert into `.env`.
///
/// # Errors
///
/// Returns [`crate::InstallEnvError`] when wipe is required, the segment is invalid, or IO fails.
pub fn run_install_write(
    options: &InstallWriteOptions,
) -> Result<InstallWriteResult, InstallEnvError> {
    let root = shop_root();
    if existing_prefix_needs_wipe(&root) && !options.wipe_confirmed {
        return Err(InstallEnvError::WipeRequired);
    }

    let prefix = match &options.admin_folder {
        Some(raw) => AdminApiPrefix::parse(raw).map_err(InstallEnvError::InvalidPrefix)?,
        None => AdminApiPrefix::parse(&random_opaque_segment())
            .map_err(InstallEnvError::InvalidPrefix)?,
    };
    let token = random_opaque_segment();
    let path = env_file_path(&root);
    upsert_env_file(
        &path,
        &[
            (ADMIN_API_PREFIX_ENV, prefix.as_str()),
            (ADMIN_TOKEN_ENV, token.as_str()),
        ],
    )?;

    // Safety: ASCII keys; generated alphanumeric values.
    unsafe {
        std::env::set_var(ADMIN_API_PREFIX_ENV, prefix.as_str());
        std::env::set_var(ADMIN_TOKEN_ENV, &token);
    }

    Ok(InstallWriteResult {
        admin_prefix: prefix.as_str().to_owned(),
        admin_token: token,
        env_path: path,
    })
}

/// Cryptographically opaque segment (`[a-z0-9]{20}`).
#[must_use]
pub fn random_opaque_segment() -> String {
    let mut bytes = [0_u8; 20];
    getrandom::fill(&mut bytes).expect("getrandom");
    bytes
        .iter()
        .map(|b| {
            const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            ALPHABET[usize::from(*b) % ALPHABET.len()] as char
        })
        .collect()
}

fn read_env_value(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            return Some(v.trim().trim_matches('"').to_owned());
        }
    }
    None
}

fn upsert_env_file(path: &Path, pairs: &[(&str, &str)]) -> Result<(), std::io::Error> {
    let mut contents = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    for &(key, value) in pairs {
        contents = upsert_key(&contents, key, value);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(path, contents)?;
    Ok(())
}

fn upsert_key(contents: &str, key: &str, value: &str) -> String {
    let mut out = String::new();
    let mut replaced = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        let existing_key = trimmed.split_once('=').map(|(k, _)| k.trim());
        if existing_key == Some(key) {
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !replaced {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn upsert_replaces_and_appends() {
        let text = upsert_key("FOO=1\n", "BAR", "2");
        assert!(text.contains("FOO=1"));
        assert!(text.contains("BAR=2"));
        let text2 = upsert_key(&text, "FOO", "9");
        assert!(text2.contains("FOO=9"));
        assert!(!text2.contains("FOO=1"));
    }

    #[test]
    fn write_requires_wipe_when_prefix_set() {
        let _guard = INSTALL_PROCESS_ENV_LOCK.lock().expect("lock");
        let dir = std::env::temp_dir().join(format!("rustashop-env-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let env_path = dir.join(".env");
        fs::write(&env_path, "RUSTASHOP_ADMIN_API_PREFIX=alreadyset1\n").expect("write");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::remove_var(ENV_FILE_ENV);
        }
        let err = run_install_write(&InstallWriteOptions {
            admin_folder: None,
            wipe_confirmed: false,
        })
        .expect_err("wipe");
        assert!(matches!(err, InstallEnvError::WipeRequired));
        let ok = run_install_write(&InstallWriteOptions {
            admin_folder: Some("newfolderok1".into()),
            wipe_confirmed: true,
        })
        .expect("write");
        assert_eq!(ok.admin_prefix, "newfolderok1");
        let body = fs::read_to_string(&env_path).expect("read");
        assert!(body.contains("RUSTASHOP_ADMIN_API_PREFIX=newfolderok1"));
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_env_skips_comments_and_blank_lines() {
        let contents = "# comment\n\nFOO=1\nBAR=\"quoted\"\nnotakey\n";
        assert_eq!(read_env_value(contents, "FOO").as_deref(), Some("1"));
        assert_eq!(read_env_value(contents, "BAR").as_deref(), Some("quoted"));
        assert_eq!(read_env_value(contents, "missing"), None);
    }

    #[test]
    fn random_opaque_segment_is_alphanumeric() {
        let segment = random_opaque_segment();
        assert_eq!(segment.len(), 20);
        assert!(segment.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn write_generates_prefix_when_unset() {
        let _guard = INSTALL_PROCESS_ENV_LOCK.lock().expect("lock");
        let dir = std::env::temp_dir().join(format!("rustashop-env-gen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::remove_var(ENV_FILE_ENV);
        }
        let ok = run_install_write(&InstallWriteOptions::default()).expect("write");
        assert_eq!(ok.admin_prefix.len(), 20);
        assert!(existing_prefix_needs_wipe(&dir));
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_prefix_needs_wipe_false_when_missing_or_default() {
        let dir =
            std::env::temp_dir().join(format!("rustashop-env-wipe-false-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        assert!(!existing_prefix_needs_wipe(&dir));
        fs::write(
            dir.join(".env"),
            format!("{ADMIN_API_PREFIX_ENV}={DEFAULT_ADMIN_API_PREFIX}\n"),
        )
        .expect("write");
        assert!(!existing_prefix_needs_wipe(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_install_write_rejects_invalid_folder() {
        let _guard = INSTALL_PROCESS_ENV_LOCK.lock().expect("lock");
        let dir =
            std::env::temp_dir().join(format!("rustashop-env-bad-folder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::remove_var(ENV_FILE_ENV);
        }
        let err = run_install_write(&InstallWriteOptions {
            admin_folder: Some("carts".into()),
            wipe_confirmed: false,
        })
        .expect_err("invalid");
        assert!(matches!(err, InstallEnvError::InvalidPrefix(_)));
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_env_file_creates_parent_and_appends_missing_newline() {
        let dir = std::env::temp_dir().join(format!(
            "rustashop-env-nested-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = dir.join("a").join("b");
        let path = nested.join(".env");
        upsert_env_file(&path, &[("FOO", "1")]).expect("write nested");
        assert!(path.is_file());
        fs::write(&path, "FOO=1").expect("no trailing newline");
        upsert_env_file(&path, &[("BAR", "2")]).expect("append");
        let body = fs::read_to_string(&path).expect("read");
        assert!(body.contains("FOO=1"));
        assert!(body.contains("BAR=2"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_install_write_maps_io_error() {
        let _guard = INSTALL_PROCESS_ENV_LOCK.lock().expect("lock");
        let dir = std::env::temp_dir().join(format!("rustashop-env-io-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let blocker = dir.join("not-a-dir");
        fs::write(&blocker, "x").expect("file");
        let env_path = blocker.join(".env");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::set_var(ENV_FILE_ENV, &env_path);
        }
        let err = run_install_write(&InstallWriteOptions {
            admin_folder: Some("newfolderok1".into()),
            wipe_confirmed: false,
        })
        .expect_err("io");
        assert!(matches!(err, InstallEnvError::Io(_)));
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
            std::env::remove_var(ENV_FILE_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
