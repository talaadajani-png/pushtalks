//! The only module allowed to touch the network (PRD §10, CLAUDE.md).
//! Downloads pinned model files with resume, checks SHA-256, and installs them.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

use super::manifest::{ModelFile, ModelSpec};

/// Progress for one download: bytes received so far out of `total`.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub received: u64,
    pub total: u64,
}

/// Hashes a file in 1 MiB blocks.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Checks every installed file of `spec` against its pinned size and SHA-256.
/// Returns the list of problems; empty means the model is safe to load.
pub fn verify(spec: &ModelSpec, models_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for f in spec.files {
        let path = spec.file_path(models_dir, f.name);
        match check_file(&path, f) {
            Ok(()) => {}
            Err(e) => problems.push(format!("{}: {e}", f.name)),
        }
    }
    problems
}

/// Cheap check that every file is present at its pinned size, without hashing. For UI state
/// only: loading a model always runs the full SHA-256 [`verify`] first (PRD §7.3).
pub fn present(spec: &ModelSpec, models_dir: &Path) -> bool {
    spec.files
        .iter()
        .all(|f| fs::metadata(spec.file_path(models_dir, f.name)).is_ok_and(|m| m.len() == f.size))
}

fn check_file(path: &Path, f: &ModelFile) -> Result<()> {
    let meta = fs::metadata(path).map_err(|_| anyhow!("missing"))?;
    if meta.len() != f.size {
        bail!("size {} != pinned {}", meta.len(), f.size);
    }
    let actual = sha256_file(path)?;
    if actual != f.sha256 {
        bail!("SHA-256 mismatch");
    }
    Ok(())
}

/// Downloads (resuming a partial file when possible), verifies and installs `spec`.
/// Skips work for files that already verify.
pub fn install(
    spec: &ModelSpec,
    models_dir: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<()> {
    if verify(spec, models_dir).is_empty() {
        return Ok(());
    }
    let staging = models_dir.join(".download");
    fs::create_dir_all(&staging)?;
    let download = staging.join(format!("{}.download", spec.id));

    let have = fs::metadata(&download).map(|m| m.len()).unwrap_or(0);
    let complete = have == spec.download_size && sha256_file(&download)? == spec.download_sha256;
    if !complete {
        fetch(spec.url, &download, spec.download_size, &mut on_progress)?;
        let actual = sha256_file(&download)?;
        if actual != spec.download_sha256 {
            fs::remove_file(&download).ok();
            bail!("{}: downloaded file failed its SHA-256 check", spec.id);
        }
    }

    let dest = spec.install_dir(models_dir);
    fs::create_dir_all(&dest)?;
    match spec.archive_root {
        Some(root) => extract(&download, root, spec.files, &dest)?,
        None => {
            let only = spec
                .files
                .first()
                .ok_or_else(|| anyhow!("{}: no files listed", spec.id))?;
            fs::copy(&download, dest.join(only.name))?;
        }
    }

    let problems = verify(spec, models_dir);
    if !problems.is_empty() {
        bail!(
            "{}: installed files failed verification: {}",
            spec.id,
            problems.join("; ")
        );
    }
    fs::remove_file(&download).ok();
    Ok(())
}

/// HTTP GET with a Range header to resume an interrupted download.
fn fetch(url: &str, path: &Path, total: u64, on_progress: &mut impl FnMut(Progress)) -> Result<()> {
    let mut have = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if have > total {
        fs::remove_file(path)?;
        have = 0;
    }
    let mut req = ureq::get(url);
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let mut resp = req.call().with_context(|| format!("download {url}"))?;
    let status = resp.status().as_u16();
    // 206 = resumed; 200 = server sent the whole file, so start over.
    let append = status == 206 && have > 0;
    if !append {
        have = 0;
    }
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)?;
    let mut reader = resp.body_mut().as_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut received = have;
    let mut last_report = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        out.write_all(&buf[..n])?;
        received += n as u64;
        if received - last_report >= 1 << 20 || received == total {
            last_report = received;
            on_progress(Progress { received, total });
        }
    }
    out.flush()?;
    Ok(())
}

/// Extracts only the pinned files from a .tar.bz2, flattening `root/name` to `dest/name`.
fn extract(archive: &Path, root: &str, files: &[ModelFile], dest: &Path) -> Result<()> {
    let wanted: Vec<(String, PathBuf)> = files
        .iter()
        .map(|f| (format!("{root}/{}", f.name), dest.join(f.name)))
        .collect();
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(File::open(archive)?));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string();
        if let Some((_, out)) = wanted.iter().find(|(w, _)| *w == name) {
            let tmp = out.with_extension("part");
            let mut f = File::create(&tmp)?;
            io::copy(&mut entry, &mut f)?;
            fs::rename(&tmp, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_files() {
        let dir = std::env::temp_dir().join(format!("pushtalks-hash-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.txt");
        fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_rejects_tampered_files() {
        let dir = std::env::temp_dir().join(format!("pushtalks-verify-{}", std::process::id()));
        let spec = super::super::manifest::SILERO_VAD;
        let path = spec.file_path(&dir, "silero_vad.onnx");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Missing file.
        assert!(!verify(&spec, &dir).is_empty());
        // Right size, wrong content.
        fs::write(&path, vec![0u8; spec.files[0].size as usize]).unwrap();
        let problems = verify(&spec, &dir);
        assert!(problems[0].contains("SHA-256"), "{problems:?}");
    }
}
