//! Self-update from GitHub Releases, with the same contract as Taskline v1.1/v1.2.
//!
//! Ask the GitHub API for the latest release, compare versions, download `taskline.exe`
//! and verify its SHA256 against the release's `SHA256SUMS.txt`. Only a verified file is
//! ever installed: a small batch script, started detached, waits for Taskline to exit,
//! renames the current exe to `taskline.exe.bak`, moves the new one into place (restoring
//! the backup if that fails) and starts Taskline again. The asset names never change, so
//! a v1.2 build updates to this one and this one updates to the next.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const REPO: &str = "DanielPuertaBernal/taskline";
pub const API_URL: &str = "https://api.github.com/repos/DanielPuertaBernal/taskline/releases/latest";
pub const EXE_ASSET: &str = "taskline.exe";
pub const SUMS_ASSET: &str = "SHA256SUMS.txt";
pub const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// The update could not be prepared safely; nothing was installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UpdateError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub exe_url: String,
    pub sums_url: String,
}

/// `v1.2.3` or `1.2.3` -> (1, 2, 3). Pre-releases and anything else -> `None`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(new), Some(old)) => new > old,
        _ => false,
    }
}

/// The latest published release from the API payload, or `None` when it is a draft, a
/// pre-release, has an odd tag, or lacks the exe or the checksums.
pub fn release_from_json(payload: &[u8]) -> Option<Release> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let object = value.as_object()?;
    let flag = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let tag = object.get("tag_name")?.as_str()?.to_owned();
    parse_version(&tag)?;
    let mut assets = HashMap::new();
    for asset in object
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(name), Some(url)) = (
            asset.get("name").and_then(Value::as_str),
            asset.get("browser_download_url").and_then(Value::as_str),
        ) && !url.is_empty()
        {
            assets.insert(name.to_owned(), url.to_owned());
        }
    }
    Some(Release {
        version: tag.trim_start_matches('v').to_owned(),
        exe_url: assets.get(EXE_ASSET)?.clone(),
        sums_url: assets.get(SUMS_ASSET)?.clone(),
        tag,
    })
}

/// `sha256sum` output: `<hex>  <name>` (a `*` binary marker is allowed).
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    let mut sums = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((hash, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            let name = name.trim().trim_start_matches('*').trim();
            sums.insert(name.to_owned(), hash.to_lowercase());
        }
    }
    sums
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().fold(String::with_capacity(64), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Checks `data` against the `taskline.exe` line of `SHA256SUMS.txt`.
pub fn verify(data: &[u8], sums_text: &str) -> Result<(), UpdateError> {
    let sums = parse_sums(sums_text);
    let expected = sums
        .get(EXE_ASSET)
        .ok_or_else(|| UpdateError(format!("{SUMS_ASSET} has no checksum for {EXE_ASSET}")))?;
    let actual = sha256_hex(data);
    if &actual != expected {
        return Err(UpdateError(format!(
            "checksum mismatch for {EXE_ASSET}: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

/// Something that downloads a URL (the app uses HTTPS; tests use a map).
pub type Getter<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;

/// The newer release, if any. Network failures mean "no update", so the app never
/// bothers the user about being offline.
pub fn find_update(current: &str, api_url: &str, get: Getter) -> Option<Release> {
    let release = release_from_json(&get(api_url).ok()?)?;
    is_newer(&release.version, current).then_some(release)
}

/// Downloads the release exe into `directory` and verifies it. Returns the verified
/// `taskline.exe.new`; on any failure nothing is left behind.
pub fn download_verified(release: &Release, directory: &Path, get: Getter) -> Result<PathBuf, UpdateError> {
    let sums =
        get(&release.sums_url).map_err(|e| UpdateError(format!("could not download {SUMS_ASSET}: {e}")))?;
    let sums = String::from_utf8_lossy(&sums).into_owned();
    if !parse_sums(&sums).contains_key(EXE_ASSET) {
        return Err(UpdateError(format!(
            "{SUMS_ASSET} has no checksum for {EXE_ASSET}"
        )));
    }
    let data =
        get(&release.exe_url).map_err(|e| UpdateError(format!("could not download {EXE_ASSET}: {e}")))?;
    verify(&data, &sums)?;
    let target = directory.join(format!("{EXE_ASSET}.new"));
    let write = || -> std::io::Result<()> {
        let (tmp, mut file) = crate::store::tempfile_in(directory, ".taskline.", ".download")?;
        let result = std::io::Write::write_all(&mut file, &data).and_then(|()| file.sync_all());
        drop(file);
        match result.and_then(|()| std::fs::rename(&tmp, &target)) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                Err(error)
            }
        }
    };
    write().map_err(|e| {
        UpdateError(format!(
            "could not write the update to {}: {e}",
            directory.display()
        ))
    })?;
    Ok(target)
}

fn bat_quote(path: &Path) -> String {
    // Inside a batch file a literal % must be doubled.
    path.display().to_string().replace('%', "%%")
}

/// The batch file that swaps `new` into `target` once `pids` exit (CRLF line ends).
pub fn swap_script(target: &Path, new: &Path, pids: &[u32]) -> String {
    let log = target.with_file_name("taskline-update.log");
    let mut lines = vec![
        "@echo off".to_owned(),
        "setlocal".to_owned(),
        format!("set \"TARGET={}\"", bat_quote(target)),
        format!("set \"NEW={}\"", bat_quote(new)),
        format!("set \"LOG={}\"", bat_quote(&log)),
    ];
    for (index, pid) in pids.iter().enumerate() {
        // Wait up to ~60 s for each process; ping is the sleep that works without a
        // console (timeout.exe refuses redirected input).
        lines.extend([
            format!("set /a TRIES{index}=0"),
            format!(":wait{index}"),
            format!("tasklist /FI \"PID eq {pid}\" /NH 2>nul | find \" {pid} \" >nul || goto waited{index}"),
            format!("set /a TRIES{index}+=1"),
            format!("if %TRIES{index}% geq 60 goto waited{index}"),
            "ping -n 2 127.0.0.1 >nul".to_owned(),
            format!("goto wait{index}"),
            format!(":waited{index}"),
        ]);
    }
    lines.extend(
        [
            "set /a MOVES=0",
            ":backup",
            "if exist \"%TARGET%.bak\" del /F /Q \"%TARGET%.bak\" >nul 2>&1",
            "move /Y \"%TARGET%\" \"%TARGET%.bak\" >nul 2>&1 && goto install",
            "set /a MOVES+=1",
            "if %MOVES% geq 20 goto fail",
            "ping -n 2 127.0.0.1 >nul",
            "goto backup",
            ":install",
            "move /Y \"%NEW%\" \"%TARGET%\" >nul 2>&1 && goto done",
            "move /Y \"%TARGET%.bak\" \"%TARGET%\" >nul 2>&1",
            ":fail",
            "echo %DATE% %TIME% update failed, kept the current version>>\"%LOG%\"",
            "if exist \"%NEW%\" del /F /Q \"%NEW%\" >nul 2>&1",
            "start \"\" \"%TARGET%\"",
            "goto end",
            ":done",
            "echo %DATE% %TIME% updated>>\"%LOG%\"",
            "start \"\" \"%TARGET%\"",
            ":end",
            // Delete this script; the trailing goto keeps cmd from reading it afterwards.
            "(goto) 2>nul & del \"%~f0\"",
        ]
        .map(str::to_owned),
    );
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "8d2a9cd4f3f2bb0e1f3e4c1d2b3a4f5e6d7c8b9a0f1e2d3c4b5a69788796a5b4";

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("2.0.0"), Some((2, 0, 0)));
        for bad in ["v1.2", "1.2.3-rc1", "1.2.3.4", "latest", "", "v1..2"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer("2.0.0", "1.2.0"));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("1.1.9", "1.2.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    fn payload(tag: &str, extra: &str) -> String {
        format!(
            r#"{{"tag_name": "{tag}"{extra}, "assets": [
                {{"name": "taskline.exe", "browser_download_url": "https://x/taskline.exe"}},
                {{"name": "SHA256SUMS.txt", "browser_download_url": "https://x/SHA256SUMS.txt"}},
                {{"name": "taskline-omarchy.tar.gz", "browser_download_url": "https://x/o.tgz"}}]}}"#
        )
    }

    #[test]
    fn release_payloads() {
        let release = release_from_json(payload("v2.0.0", "").as_bytes()).unwrap();
        assert_eq!(release.version, "2.0.0");
        assert_eq!(release.exe_url, "https://x/taskline.exe");
        assert!(release_from_json(payload("v2.0.0", r#", "prerelease": true"#).as_bytes()).is_none());
        assert!(release_from_json(payload("v2.0.0", r#", "draft": true"#).as_bytes()).is_none());
        assert!(release_from_json(payload("nightly", "").as_bytes()).is_none());
        assert!(release_from_json(br#"{"tag_name": "v2.0.0", "assets": []}"#).is_none());
        assert!(release_from_json(b"not json").is_none());
    }

    #[test]
    fn sums_parsing() {
        let sums = parse_sums(&format!(
            "{HASH}  taskline.exe\n{}  *other.tgz\njunk line\n",
            HASH.to_uppercase()
        ));
        assert_eq!(sums.get("taskline.exe").map(String::as_str), Some(HASH));
        assert_eq!(sums.get("other.tgz").map(String::as_str), Some(HASH));
        assert_eq!(sums.len(), 2);
    }

    fn fake(files: Vec<(&'static str, Vec<u8>)>) -> impl Fn(&str) -> Result<Vec<u8>, String> {
        move |url: &str| {
            files
                .iter()
                .find(|(u, _)| *u == url)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| format!("404 {url}"))
        }
    }

    fn release() -> Release {
        release_from_json(payload("v99.0.0", "").as_bytes()).unwrap()
    }

    #[test]
    fn verified_download_is_installed_and_tampered_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let exe = b"MZ new build".to_vec();
        let sums = format!("{}  taskline.exe\n", sha256_hex(&exe)).into_bytes();
        let get = fake(vec![
            ("https://x/SHA256SUMS.txt", sums.clone()),
            ("https://x/taskline.exe", exe.clone()),
        ]);
        let new = download_verified(&release(), dir.path(), &get).unwrap();
        assert_eq!(new.file_name().unwrap(), "taskline.exe.new");
        assert_eq!(std::fs::read(&new).unwrap(), exe);
        std::fs::remove_file(&new).unwrap();

        let tampered = fake(vec![
            ("https://x/SHA256SUMS.txt", sums),
            ("https://x/taskline.exe", b"evil".to_vec()),
        ]);
        let error = download_verified(&release(), dir.path(), &tampered).unwrap_err();
        assert!(error.0.contains("checksum mismatch"), "{error}");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "nothing left behind"
        );

        let no_line = fake(vec![
            ("https://x/SHA256SUMS.txt", b"abc  other\n".to_vec()),
            ("https://x/taskline.exe", exe.clone()),
        ]);
        assert!(
            download_verified(&release(), dir.path(), &no_line)
                .unwrap_err()
                .0
                .contains("no checksum")
        );
        let offline = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
        assert!(download_verified(&release(), dir.path(), &offline).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn find_update_ignores_older_and_offline() {
        let api = |tag: &'static str| fake(vec![("api", payload(tag, "").into_bytes())]);
        assert_eq!(
            find_update("1.2.0", "api", &api("v2.0.0")).unwrap().version,
            "2.0.0"
        );
        assert!(find_update("2.0.0", "api", &api("v2.0.0")).is_none());
        assert!(find_update("2.0.0", "api", &api("v1.2.0")).is_none());
        let offline = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
        assert!(find_update("1.0.0", "api", &offline).is_none());
    }

    #[test]
    fn swap_script_backs_up_installs_and_relaunches() {
        let script = swap_script(
            Path::new(r"C:\Apps\100%\taskline.exe"),
            Path::new(r"C:\Apps\100%\taskline.exe.new"),
            &[42],
        );
        assert!(script.contains("set \"TARGET=C:\\Apps\\100%%\\taskline.exe\"\r\n"));
        assert!(script.contains("tasklist /FI \"PID eq 42\""));
        assert!(script.contains("move /Y \"%TARGET%\" \"%TARGET%.bak\""));
        assert!(script.contains("move /Y \"%NEW%\" \"%TARGET%\" >nul 2>&1 && goto done"));
        assert!(
            script.contains("move /Y \"%TARGET%.bak\" \"%TARGET%\""),
            "rollback"
        );
        assert_eq!(
            script.matches("start \"\" \"%TARGET%\"").count(),
            2,
            "relaunch on success and failure"
        );
        assert!(!script.replace("\r\n", "").contains('\n'), "CRLF only");
    }
}
