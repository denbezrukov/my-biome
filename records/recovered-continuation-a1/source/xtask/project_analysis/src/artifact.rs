use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactIdentity {
    pub package: String,
    pub version: String,
    pub integrity: String,
    pub dependency_lock_digest: String,
    pub runtime_version: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ArtifactMismatch;

pub fn validate_artifact(
    expected: &ArtifactIdentity,
    observed: &ArtifactIdentity,
) -> Result<(), ArtifactMismatch> {
    if expected == observed {
        Ok(())
    } else {
        Err(ArtifactMismatch)
    }
}

pub fn artifact_integrity(bytes: &[u8]) -> String {
    format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)))
}

pub fn verify_bytes(bytes: &[u8], integrity: &str) -> Result<(), ArtifactMismatch> {
    if artifact_integrity(bytes) == integrity {
        Ok(())
    } else {
        Err(ArtifactMismatch)
    }
}

impl std::fmt::Display for ArtifactMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("oracle artifact identity or integrity mismatch; execution refused")
    }
}
impl std::error::Error for ArtifactMismatch {}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

pub fn supported_runtime(version: &str) -> bool {
    let parts: Vec<_> = version
        .trim_start_matches('v')
        .split('.')
        .map(str::parse::<u32>)
        .collect();
    match parts.as_slice() {
        [Ok(20), Ok(minor), Ok(_)] => *minor >= 19,
        [Ok(22), Ok(minor), Ok(_)] => *minor >= 12,
        [Ok(major), Ok(_), Ok(_)] => *major > 22,
        _ => false,
    }
}

pub struct InstalledOracle {
    pub identity: ArtifactIdentity,
    pub node: std::path::PathBuf,
    pub executable: std::path::PathBuf,
    pub installation: serde_json::Value,
}

/// Verifies the downloaded archive before npm installation or oracle execution.
/// npm independently verifies the locked dependency archives, with lifecycle scripts disabled.
pub fn install(
    root: &std::path::Path,
    artifact: Option<&std::path::Path>,
) -> anyhow::Result<InstalledOracle> {
    use anyhow::{Context, ensure};
    use std::{fs, process::Command};
    let oracle = root.join("oracle");
    let expected: ArtifactIdentity =
        serde_json::from_slice(&fs::read(oracle.join("identity.json"))?)?;
    let archive = artifact.map_or_else(
        || root.join("cache/knip-6.43.0.tgz"),
        std::path::Path::to_path_buf,
    );
    if !archive.exists() && artifact.is_none() {
        fs::create_dir_all(root.join("cache"))?;
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "https://registry.npmjs.org/knip/-/knip-6.43.0.tgz",
                "--output",
            ])
            .arg(&archive)
            .status()?;
        ensure!(status.success(), "artifact download failed");
    }
    let bytes = fs::read(&archive).context("read pinned archive")?;
    verify_bytes(&bytes, &expected.integrity)?;
    let lock_bytes = fs::read(oracle.join("package-lock.json"))?;
    let lock: serde_json::Value = serde_json::from_slice(&lock_bytes)?;
    ensure!(
        lock["packages"]["node_modules/knip"]["integrity"] == expected.integrity,
        "Knip lock integrity differs from identity"
    );
    ensure!(
        lock["packages"]["node_modules/knip"]["version"] == expected.version,
        "Knip lock version differs from identity"
    );
    let runtime = Command::new("node")
        .arg("--version")
        .env_remove("NODE_OPTIONS")
        .output()?;
    ensure!(runtime.status.success(), "Node version command failed");
    let runtime_version = String::from_utf8(runtime.stdout)?.trim().to_owned();
    ensure!(
        supported_runtime(&runtime_version),
        "Node does not satisfy ^20.19.0 || >=22.12.0"
    );
    let observed = ArtifactIdentity {
        package: "knip".into(),
        version: expected.version.clone(),
        integrity: artifact_integrity(&bytes),
        dependency_lock_digest: sha256(&lock_bytes),
        runtime_version,
    };
    validate_artifact(&expected, &observed)?;
    let node_path = Command::new("node")
        .args(["-p", "process.execPath"])
        .env_remove("NODE_OPTIONS")
        .output()?;
    ensure!(
        node_path.status.success(),
        "Node executable discovery failed"
    );
    let node = std::path::PathBuf::from(String::from_utf8(node_path.stdout)?.trim());
    let args = [
        "ci",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--workspaces=false",
    ];
    let output = Command::new("npm")
        .args(args)
        .current_dir(&oracle)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .output()?;
    let installation = serde_json::json!({"command": ["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund", "--workspaces=false"], "cwd": oracle, "exit_code": output.status.code(), "stdout": String::from_utf8_lossy(&output.stdout), "stderr": String::from_utf8_lossy(&output.stderr)});
    ensure!(
        output.status.success(),
        "isolated npm ci failed: {installation}"
    );
    let package: serde_json::Value =
        serde_json::from_slice(&fs::read(oracle.join("node_modules/knip/package.json"))?)?;
    ensure!(
        package["name"] == observed.package && package["version"] == observed.version,
        "installed package identity mismatch"
    );
    let executable = oracle.join("node_modules/knip/bin/knip.js");
    ensure!(executable.is_file(), "locked oracle executable missing");
    Ok(InstalledOracle {
        identity: observed,
        node,
        executable,
        installation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ArtifactIdentity {
        ArtifactIdentity {
            package: "knip".into(),
            version: "6.43.0".into(),
            integrity: "sha512-original".into(),
            dependency_lock_digest: "lock".into(),
            runtime_version: "v24.19.0".into(),
        }
    }

    #[test]
    fn same_version_different_integrity_is_rejected() {
        let expected = identity();
        let mut observed = expected.clone();
        observed.integrity = "sha512-corrupted".into();
        assert_ne!(expected.integrity, observed.integrity);
        assert!(validate_artifact(&expected, &observed).is_err());
    }

    #[test]
    fn artifact_identity_includes_lock_and_runtime() {
        let expected = identity();
        assert!(validate_artifact(&expected, &expected).is_ok());
        let mut observed = expected.clone();
        observed.dependency_lock_digest = "another lock".into();
        assert!(validate_artifact(&expected, &observed).is_err());
        observed = expected.clone();
        observed.runtime_version = "v22.12.0".into();
        assert!(validate_artifact(&expected, &observed).is_err());
    }

    #[test]
    fn corrupt_artifact_bytes_are_rejected_before_execution() {
        let integrity = artifact_integrity(b"original tarball");
        assert!(verify_bytes(b"original tarball", &integrity).is_ok());
        assert!(verify_bytes(b"corrupted tarball", &integrity).is_err());
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::supported_runtime;

    #[test]
    fn runtime_boundaries_match_the_pinned_package() {
        for version in ["v20.19.0", "v20.20.0", "v22.12.0", "v23.0.0", "v24.19.0"] {
            assert!(supported_runtime(version), "{version}");
        }
        for version in ["v20.18.0", "v21.9.0", "v22.11.9", "garbage", "v20"] {
            assert!(!supported_runtime(version), "{version}");
        }
    }
}
