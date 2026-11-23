//! Unified folder, archive, and snapshot source backend for Folder Compare.
//!
//! Supports:
//! - Live filesystem directories (`WalkDir`)
//! - ZIP-family archives (`.zip`, `.jar`, `.apk`, `.xpi`, `.nupkg`, `.war`, `.ear`, `.cbz`) via `zip`
//! - TAR & gzipped TAR archives (`.tar`, `.tgz`, `.tar.gz`) via pure-Rust USTAR/PAX/GNU reader + `flate2`
//! - Extended archives (`.7z`, `.tar.xz`, `.txz`, `.tar.bz2`, `.tbz2`, `.tar.zst`, `.tzst`) via Windows `bsdtar`
//! - Folder Snapshots (`.acsnap.json`, `.bcss`) with per-file `crc32fast` checksums

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;
use zip::ZipArchive;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Default)]
pub struct EntryMeta {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub crc32: Option<u32>,
    /// Windows DOS attribute bits (`FILE_ATTRIBUTE_*`: R=0x1, H=0x2, S=0x4, A=0x20).
    pub dos_attrs: u32,
    /// POSIX mode bits (`st_mode & 0o7777`).
    pub unix_mode: Option<u32>,
    pub unix_uid: Option<u32>,
    pub unix_gid: Option<u32>,
}

impl EntryMeta {
    pub fn from_fs(meta: &fs::Metadata) -> Self {
        #[cfg(windows)]
        let dos_attrs = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes()
        };
        #[cfg(not(windows))]
        let dos_attrs = if meta.permissions().readonly() { 0x1 } else { 0 };

        #[cfg(unix)]
        let (unix_mode, unix_uid, unix_gid) = {
            use std::os::unix::fs::MetadataExt;
            (Some(meta.mode() & 0o7777), Some(meta.uid()), Some(meta.gid()))
        };
        #[cfg(not(unix))]
        let (unix_mode, unix_uid, unix_gid) = (None, None, None);

        Self {
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            crc32: None,
            dos_attrs,
            unix_mode,
            unix_uid,
            unix_gid,
        }
    }

    pub fn dir() -> Self {
        Self {
            is_dir: true,
            ..Self::default()
        }
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    pub fn len(&self) -> u64 {
        self.size
    }

    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Directory,
    Zip,
    Tar,
    TarGz,
    ExternalArchive,
    Snapshot,
}

fn lower_file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn source_kind(path: &Path) -> Option<SourceKind> {
    if path.is_dir() {
        return Some(SourceKind::Directory);
    }
    let name = lower_file_name(path);
    if name.ends_with(".acsnap.json") || name.ends_with(".bcss") {
        return Some(SourceKind::Snapshot);
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(SourceKind::TarGz);
    }
    if name.ends_with(".tar") {
        return Some(SourceKind::Tar);
    }
    if name.ends_with(".tar.xz")
        || name.ends_with(".txz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tzst")
        || name.ends_with(".7z")
    {
        return Some(SourceKind::ExternalArchive);
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        ext.as_str(),
        "zip" | "jar" | "apk" | "xpi" | "nupkg" | "war" | "ear" | "cbz"
    ) {
        return Some(SourceKind::Zip);
    }
    None
}

pub fn is_archive_path(path: &Path) -> bool {
    matches!(
        source_kind(path),
        Some(
            SourceKind::Zip | SourceKind::Tar | SourceKind::TarGz | SourceKind::ExternalArchive
        )
    )
}

pub fn is_snapshot_path(path: &Path) -> bool {
    source_kind(path) == Some(SourceKind::Snapshot)
}

pub fn is_folder_like(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    path.is_file() && source_kind(path).is_some()
}

pub fn is_writable(path: &Path) -> bool {
    path.is_dir()
}

/// Sanitize an archive internal path so `..`, drive prefixes, and root slashes can never escape.
fn sanitize_relative_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().replace('\\', "/");
    let mut out = PathBuf::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') || part.contains('\0') {
            return None;
        }
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn ensure_parent_dirs(entries: &mut BTreeMap<PathBuf, EntryMeta>, rel: &Path) {
    let mut cur = rel.parent();
    while let Some(parent) = cur {
        if parent.as_os_str().is_empty() {
            break;
        }
        entries
            .entry(parent.to_path_buf())
            .or_insert_with(EntryMeta::dir);
        cur = parent.parent();
    }
}

pub fn scan_source(
    root: &Path,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    scan_source_with_options(root, false, cancel, errors)
}

pub fn scan_source_with_options(
    root: &Path,
    follow_symlinks: bool,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    match source_kind(root) {
        Some(SourceKind::Directory) => scan_directory(root, follow_symlinks, cancel, errors),
        Some(SourceKind::Zip) => scan_zip(root, cancel, errors),
        Some(SourceKind::Tar) => scan_tar(root, false, cancel, errors),
        Some(SourceKind::TarGz) => scan_tar(root, true, cancel, errors),
        Some(SourceKind::ExternalArchive) => scan_external_archive(root, cancel, errors),
        Some(SourceKind::Snapshot) => scan_snapshot(root, errors),
        None => {
            errors.push(crate::i18n::tr_fmt(
                "folder.unsupported_source",
                &[("path", &root.display().to_string())],
            ));
            BTreeMap::new()
        }
    }
}

fn scan_directory(
    root: &Path,
    follow_symlinks: bool,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    let mut entries = BTreeMap::new();
    for result in WalkDir::new(root).follow_links(follow_symlinks).min_depth(1) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match result {
            Ok(entry) => match entry.metadata() {
                Ok(metadata) => {
                    if let Ok(relative) = entry.path().strip_prefix(root) {
                        entries.insert(relative.to_owned(), EntryMeta::from_fs(&metadata));
                    }
                }
                Err(error) => errors.push(error.to_string()),
            },
            Err(error) => errors.push(error.to_string()),
        }
    }
    entries
}

fn scan_zip(
    root: &Path,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    let mut entries = BTreeMap::new();
    let file = match fs::File::open(root) {
        Ok(f) => f,
        Err(e) => {
            errors.push(format!("{}: {e}", root.display()));
            return entries;
        }
    };
    let mut archive = match ZipArchive::new(BufReader::new(file)) {
        Ok(a) => a,
        Err(e) => {
            errors.push(crate::i18n::tr_fmt(
                "folder.archive_open_error",
                &[
                    ("path", &root.display().to_string()),
                    ("error", &e.to_string()),
                ],
            ));
            return entries;
        }
    };
    for i in 0..archive.len() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entry) = archive.by_index_raw(i) else {
            continue;
        };
        let Ok(raw_name) = entry.name() else {
            continue;
        };
        let is_dir = raw_name.ends_with('/') || raw_name.ends_with('\\');
        let Some(rel) = sanitize_relative_path(&raw_name) else {
            continue;
        };
        ensure_parent_dirs(&mut entries, &rel);
        let modified = entry.last_modified().and_then(dos_datetime_to_system_time);
        let meta = EntryMeta {
            is_dir,
            size: if is_dir { 0 } else { entry.size() },
            modified,
            crc32: if is_dir { None } else { Some(entry.crc32()) },
            ..EntryMeta::default()
        };
        entries.insert(rel, meta);
    }
    entries
}

fn dos_datetime_to_system_time(dt: zip::DateTime) -> Option<SystemTime> {
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::from(dt.year()),
        u32::from(dt.month()),
        u32::from(dt.day()),
    )?;
    let time = chrono::NaiveTime::from_hms_opt(
        u32::from(dt.hour()),
        u32::from(dt.minute()),
        u32::from(dt.second()),
    )?;
    let ndt = chrono::NaiveDateTime::new(date, time);
    let secs = ndt.and_utc().timestamp();
    if secs >= 0 {
        Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
    } else {
        None
    }
}

fn scan_tar(
    root: &Path,
    gzipped: bool,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    let mut entries = BTreeMap::new();
    let file = match fs::File::open(root) {
        Ok(f) => f,
        Err(e) => {
            errors.push(format!("{}: {e}", root.display()));
            return entries;
        }
    };
    let res = if gzipped {
        let mut gz = GzDecoder::new(BufReader::new(file));
        parse_tar_stream(&mut gz, cancel, &mut entries, None)
    } else {
        let mut reader = BufReader::new(file);
        parse_tar_stream(&mut reader, cancel, &mut entries, None)
    };
    if let Err(e) = res {
        errors.push(format!("{}: {e}", root.display()));
    }
    entries
}

/// Pure-Rust USTAR / GNU LongName / PAX TAR reader.
/// When `extract_target` is `Some((target_rel, out_buf))`, fills `out_buf` for that entry.
fn parse_tar_stream(
    reader: &mut impl Read,
    cancel: &AtomicBool,
    entries: &mut BTreeMap<PathBuf, EntryMeta>,
    mut extract_target: Option<(&Path, &mut Option<Vec<u8>>)>,
) -> Result<(), String> {
    let mut header = [0u8; 512];
    let mut gnu_long_name: Option<String> = None;
    let mut pax_path: Option<String> = None;
    let mut pax_size: Option<u64> = None;

    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if !read_exact_or_eof(reader, &mut header)? {
            break;
        }
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let typeflag = header[156];
        let size = pax_size
            .take()
            .unwrap_or_else(|| parse_tar_octal(&header[124..136]));
        let mtime = parse_tar_octal(&header[136..148]);
        let name_field = trim_tar_str(&header[0..100]);
        let prefix_field = trim_tar_str(&header[345..500]);
        let full_name = if let Some(p) = pax_path.take() {
            p
        } else if let Some(l) = gnu_long_name.take() {
            l
        } else if !prefix_field.is_empty() {
            format!("{prefix_field}/{name_field}")
        } else {
            name_field
        };

        let padded = size.div_ceil(512).saturating_mul(512);
        let pad_rem = padded.saturating_sub(size);

        match typeflag {
            b'L' => {
                // GNU LongLink path
                let mut buf = vec![0u8; size.min(65536) as usize];
                reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("Erro ao ler GNU LongName TAR: {e}"))?;
                skip_bytes(reader, size.saturating_sub(buf.len() as u64) + pad_rem)?;
                gnu_long_name = Some(trim_tar_str(&buf));
                continue;
            }
            b'x' | b'g' => {
                // PAX extended header
                let mut buf = vec![0u8; size.min(128 * 1024) as usize];
                reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("Erro ao ler cabeçalho PAX TAR: {e}"))?;
                skip_bytes(reader, size.saturating_sub(buf.len() as u64) + pad_rem)?;
                if typeflag == b'x' {
                    for line in String::from_utf8_lossy(&buf).lines() {
                        if let Some((_, kv)) = line.split_once(' ') {
                            if let Some((k, v)) = kv.split_once('=') {
                                match k {
                                    "path" => pax_path = Some(v.to_string()),
                                    "size" => pax_size = v.parse::<u64>().ok(),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                continue;
            }
            _ => {}
        }

        let is_dir = typeflag == b'5' || full_name.ends_with('/');
        let rel = sanitize_relative_path(&full_name);

        let want_extract = matches!(
            (&extract_target, &rel),
            (Some((target, out)), Some(r)) if out.is_none() && same_rel_path(r, target)
        );

        if want_extract && !is_dir {
            let mut data = vec![0u8; size as usize];
            reader
                .read_exact(&mut data)
                .map_err(|e| format!("Erro ao extrair entrada TAR: {e}"))?;
            skip_bytes(reader, pad_rem)?;
            if let Some((_, out)) = extract_target.as_mut() {
                **out = Some(data);
            }
            return Ok(());
        }

        let crc32 = if !is_dir && extract_target.is_none() && size <= 16 * 1024 * 1024 {
            let mut hasher = crc32fast::Hasher::new();
            let mut remaining = size;
            let mut chunk = [0u8; 32 * 1024];
            while remaining > 0 {
                let to_read = (remaining as usize).min(chunk.len());
                reader
                    .read_exact(&mut chunk[..to_read])
                    .map_err(|e| format!("Erro ao ler dados TAR: {e}"))?;
                hasher.update(&chunk[..to_read]);
                remaining -= to_read as u64;
            }
            skip_bytes(reader, pad_rem)?;
            Some(hasher.finalize())
        } else {
            skip_bytes(reader, padded)?;
            None
        };

        if let Some(rel_path) = rel {
            ensure_parent_dirs(entries, &rel_path);
            let modified = (mtime > 0).then(|| UNIX_EPOCH + Duration::from_secs(mtime));
            entries.insert(
                rel_path,
                EntryMeta {
                    is_dir,
                    size: if is_dir { 0 } else { size },
                    modified,
                    crc32,
                    ..EntryMeta::default()
                },
            );
        }
    }
    Ok(())
}

fn read_exact_or_eof(reader: &mut impl Read, buf: &mut [u8]) -> Result<bool, String> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Erro de leitura TAR: {e}")),
        }
    }
    Ok(true)
}

fn skip_bytes(reader: &mut impl Read, mut count: u64) -> Result<(), String> {
    let mut buf = [0u8; 16 * 1024];
    while count > 0 {
        let to_read = (count as usize).min(buf.len());
        let n = reader
            .read(&mut buf[..to_read])
            .map_err(|e| format!("Erro ao avançar entrada TAR: {e}"))?;
        if n == 0 {
            break;
        }
        count -= n as u64;
    }
    Ok(())
}

fn trim_tar_str(slice: &[u8]) -> String {
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

fn parse_tar_octal(slice: &[u8]) -> u64 {
    // Handle GNU base-256 binary encoding when high bit is set.
    if slice.first().is_some_and(|&b| b & 0x80 != 0) {
        let mut val = 0u64;
        for &b in &slice[1..] {
            val = (val << 8) | (b as u64);
        }
        return val;
    }
    let s = trim_tar_str(slice);
    u64::from_str_radix(s.trim(), 8).unwrap_or(0)
}

fn tar_executable() -> &'static str {
    #[cfg(windows)]
    {
        if Path::new(r"C:\Windows\System32\tar.exe").exists() {
            r"C:\Windows\System32\tar.exe"
        } else {
            "tar"
        }
    }
    #[cfg(not(windows))]
    {
        "tar"
    }
}

fn scan_external_archive(
    root: &Path,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) -> BTreeMap<PathBuf, EntryMeta> {
    let mut entries = BTreeMap::new();
    let mut cmd = Command::new(tar_executable());
    cmd.arg("-tvf").arg(root);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            errors.push(format!(
                "Não foi possível listar o arquivo compactado {}: {e}",
                root.display()
            ));
            return entries;
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        errors.push(format!(
            "Falha ao abrir arquivo compactado {}: {}",
            root.display(),
            if stderr.is_empty() {
                "formato não suportado".into()
            } else {
                stderr
            }
        ));
        return entries;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    for line in listing.lines() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Some((rel, meta)) = parse_bsdtar_verbose_line(line) {
            ensure_parent_dirs(&mut entries, &rel);
            entries.insert(rel, meta);
        }
    }
    entries
}

/// Parses `bsdtar -tvf` lines such as:
/// `-rw-r--r--  0 0      0        1234 Nov 21 12:34 dir/file.txt`
/// `drwxr-xr-x  0 0      0           0 Nov 21  2026 dir/`
fn parse_bsdtar_verbose_line(line: &str) -> Option<(PathBuf, EntryMeta)> {
    let line = line.trim_end();
    if line.len() < 20 {
        return None;
    }
    let is_dir = line.starts_with('d');
    let mut parts = line.split_whitespace();
    let _mode = parts.next()?;
    let _links = parts.next()?;
    let _user = parts.next()?;
    let _group = parts.next()?;
    let size = parts.next()?.parse::<u64>().unwrap_or(0);
    let _month = parts.next()?;
    let _day = parts.next()?;
    let _time_or_year = parts.next()?;
    // Recover original filename slice preserving spaces inside filenames.
    let remainder = skip_whitespace_tokens(line, 8)?;
    let name = remainder.split(" -> ").next().unwrap_or(remainder);
    let rel = sanitize_relative_path(name)?;
    Some((
        rel,
        EntryMeta {
            is_dir: is_dir || name.ends_with('/'),
            size: if is_dir { 0 } else { size },
            modified: None,
            crc32: None,
            ..EntryMeta::default()
        },
    ))
}

fn skip_whitespace_tokens(line: &str, count: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    for _ in 0..count {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    (i < bytes.len()).then(|| &line[i..])
}

// ============================================================================
// Folder Snapshots (`.acsnap.json` / `.bcss`)
// ============================================================================

#[derive(Serialize, Deserialize)]
struct FolderSnapshot {
    version: u32,
    created_unix: u64,
    source: String,
    entries: Vec<SnapshotEntry>,
}

#[derive(Serialize, Deserialize)]
struct SnapshotEntry {
    path: String,
    is_dir: bool,
    size: u64,
    modified_unix: Option<u64>,
    crc32: Option<u32>,
}

pub fn save_snapshot(
    root: &Path,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let mut errors = Vec::new();
    let map = scan_source(root, cancel, &mut errors);
    if !errors.is_empty() && map.is_empty() {
        return Err(errors.join("\n"));
    }
    let mut snap_entries = Vec::with_capacity(map.len());
    for (rel, mut meta) in map {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::i18n::tr("folder.snapshot_canceled"));
        }
        if meta.is_file() && meta.crc32.is_none() {
            if let Ok(bytes) = read_entry_bytes(root, &rel) {
                meta.crc32 = Some(crc32fast::hash(&bytes));
            }
        }
        let modified_unix = meta
            .modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        snap_entries.push(SnapshotEntry {
            path: rel.to_string_lossy().replace('\\', "/"),
            is_dir: meta.is_dir,
            size: meta.size,
            modified_unix,
            crc32: meta.crc32,
        });
    }
    let count = snap_entries.len();
    let created_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let snap = FolderSnapshot {
        version: 1,
        created_unix,
        source: root.to_string_lossy().into_owned(),
        entries: snap_entries,
    };
    let json = serde_json::to_vec_pretty(&snap)
        .map_err(|e| format!("Falha ao serializar instantâneo: {e}"))?;
    crate::document::atomic_write(output, &json)?;
    Ok(count)
}

fn scan_snapshot(root: &Path, errors: &mut Vec<String>) -> BTreeMap<PathBuf, EntryMeta> {
    let mut entries = BTreeMap::new();
    let bytes = match fs::read(root) {
        Ok(b) => b,
        Err(e) => {
            errors.push(format!("{}: {e}", root.display()));
            return entries;
        }
    };
    let snap: FolderSnapshot = match serde_json::from_slice(&bytes) {
        Ok(s) => s,
        Err(e) => {
            errors.push(crate::i18n::tr_fmt(
                "folder.snapshot_invalid",
                &[
                    ("path", &root.display().to_string()),
                    ("error", &e.to_string()),
                ],
            ));
            return entries;
        }
    };
    for item in snap.entries {
        if let Some(rel) = sanitize_relative_path(&item.path) {
            ensure_parent_dirs(&mut entries, &rel);
            entries.insert(
                rel,
                EntryMeta {
                    is_dir: item.is_dir,
                    size: if item.is_dir { 0 } else { item.size },
                    modified: item
                        .modified_unix
                        .map(|s| UNIX_EPOCH + Duration::from_secs(s)),
                    crc32: item.crc32,
                    ..EntryMeta::default()
                },
            );
        }
    }
    entries
}

// ============================================================================
// Content Comparison & File Materialization
// ============================================================================

pub fn same_entry_content(
    left_root: &Path,
    left_rel: &Path,
    left_meta: &EntryMeta,
    right_root: &Path,
    right_rel: &Path,
    right_meta: &EntryMeta,
    cancel: &AtomicBool,
) -> std::io::Result<bool> {
    if left_meta.size != right_meta.size {
        return Ok(false);
    }
    // Fast-path when both sides already have CRC32 (e.g. ZIP or Snapshot).
    if let (Some(a), Some(b)) = (left_meta.crc32, right_meta.crc32) {
        if a != b {
            return Ok(false);
        }
        if is_snapshot_path(left_root) || is_snapshot_path(right_root) {
            return Ok(true);
        }
    }
    // Fast streaming comparison for two live files.
    if left_root.is_dir() && right_root.is_dir() {
        return same_live_file_content(
            &left_root.join(left_rel),
            &right_root.join(right_rel),
            cancel,
        );
    }
    // Snapshot vs live file or archive: compare via CRC32.
    if is_snapshot_path(left_root) || is_snapshot_path(right_root) {
        let crc_l = match left_meta.crc32 {
            Some(c) => c,
            None => crc32fast::hash(
                &read_entry_bytes(left_root, left_rel).map_err(std::io::Error::other)?,
            ),
        };
        let crc_r = match right_meta.crc32 {
            Some(c) => c,
            None => crc32fast::hash(
                &read_entry_bytes(right_root, right_rel).map_err(std::io::Error::other)?,
            ),
        };
        return Ok(crc_l == crc_r);
    }
    let a = read_entry_bytes(left_root, left_rel).map_err(std::io::Error::other)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "app.comparison_canceled",
        ));
    }
    let b = read_entry_bytes(right_root, right_rel).map_err(std::io::Error::other)?;
    Ok(a == b)
}

fn same_live_file_content(
    left: &Path,
    right: &Path,
    cancel: &AtomicBool,
) -> std::io::Result<bool> {
    let mut left = fs::File::open(left)?;
    let mut right = fs::File::open(right)?;
    let mut a = vec![0_u8; 256 * 1024];
    let mut b = vec![0_u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "app.comparison_canceled",
            ));
        }
        let n = read_block(&mut left, &mut a)?;
        if read_block(&mut right, &mut b)? != n || a[..n] != b[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

pub(crate) fn read_block(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

pub fn read_entry_bytes(root: &Path, rel: &Path) -> Result<Vec<u8>, String> {
    match source_kind(root) {
        Some(SourceKind::Directory) => {
            fs::read(root.join(rel)).map_err(|e| format!("{}: {e}", rel.display()))
        }
        Some(SourceKind::Zip) => {
            let file = fs::File::open(root).map_err(|e| format!("{}: {e}", root.display()))?;
            let mut archive = ZipArchive::new(BufReader::new(file))
                .map_err(|e| format!("{}: {e}", root.display()))?;
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("{}: {e}", root.display()))?;
                let Ok(raw_name) = entry.name() else {
                    continue;
                };
                if let Some(candidate) = sanitize_relative_path(&raw_name) {
                    if same_rel_path(&candidate, rel) {
                        let mut buf = Vec::new();
                        entry
                            .read_to_end(&mut buf)
                            .map_err(|e| format!("{}: {e}", rel.display()))?;
                        return Ok(buf);
                    }
                }
            }
            Err(format!("Entrada não encontrada no ZIP: {}", rel.display()))
        }
        Some(SourceKind::Tar | SourceKind::TarGz) => {
            let gzipped = source_kind(root) == Some(SourceKind::TarGz);
            let file = fs::File::open(root).map_err(|e| format!("{}: {e}", root.display()))?;
            let cancel = AtomicBool::new(false);
            let mut dummy = BTreeMap::new();
            let mut out: Option<Vec<u8>> = None;
            if gzipped {
                let mut gz = GzDecoder::new(BufReader::new(file));
                parse_tar_stream(&mut gz, &cancel, &mut dummy, Some((rel, &mut out)))?;
            } else {
                let mut reader = BufReader::new(file);
                parse_tar_stream(&mut reader, &cancel, &mut dummy, Some((rel, &mut out)))?;
            }
            out.ok_or_else(|| format!("Entrada não encontrada no TAR: {}", rel.display()))
        }
        Some(SourceKind::ExternalArchive) => {
            let tar_path = rel.to_string_lossy().replace('\\', "/");
            let mut cmd = Command::new(tar_executable());
            cmd.arg("-xOf").arg(root).arg(&tar_path);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let output = cmd
                .output()
                .map_err(|e| format!("Falha ao extrair {}: {e}", rel.display()))?;
            if !output.status.success() {
                return Err(format!(
                    "Não foi possível extrair {} de {}",
                    rel.display(),
                    root.display()
                ));
            }
            Ok(output.stdout)
        }
        Some(SourceKind::Snapshot) => Err(crate::i18n::tr("folder.snapshot_no_content")),
        None => Err(crate::i18n::tr_fmt(
            "folder.unsupported_source",
            &[("path", &root.display().to_string())],
        )),
    }
}

/// Returns a readable path on disk for opening a file in a comparator.
/// For live directories, returns `root.join(rel)` directly; for archives, extracts
/// into a deterministic temp cache directory so Text/PDF/Picture/Table/Hex Compare work seamlessly.
pub fn materialize_file(root: &Path, rel: &Path) -> Result<PathBuf, String> {
    if root.is_dir() {
        return Ok(root.join(rel));
    }
    let bytes = read_entry_bytes(root, rel)?;
    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(rel.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    let file_name = rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "arquivo.bin".into());
    let dir = std::env::temp_dir().join("anycompare-archive").join(hex);
    fs::create_dir_all(&dir).map_err(|e| format!("Não foi possível criar pasta temporária: {e}"))?;
    let dest = dir.join(file_name);
    fs::write(&dest, bytes)
        .map_err(|e| format!("Não foi possível extrair {}: {e}", rel.display()))?;
    Ok(dest)
}

fn same_rel_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        p.components()
            .filter_map(|c| match c {
                Component::Normal(s) => {
                    let s = s.to_string_lossy();
                    Some(if cfg!(windows) {
                        s.to_lowercase()
                    } else {
                        s.into_owned()
                    })
                }
                _ => None,
            })
            .collect()
    };
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scans_and_reads_zip_tar_gz_and_snapshot() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ac-src-test-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        // 1. ZIP archive
        let zip_path = dir.join("pack.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("sub/ola.txt", opts).unwrap();
            zw.write_all("Olá ZIP!".as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        assert!(is_archive_path(&zip_path));
        assert!(is_folder_like(&zip_path));
        let cancel = AtomicBool::new(false);
        let mut errs = Vec::new();
        let zmap = scan_source(&zip_path, &cancel, &mut errs);
        assert!(errs.is_empty(), "zip errors: {errs:?}");
        assert!(zmap.contains_key(Path::new("sub")));
        assert!(zmap.contains_key(Path::new("sub/ola.txt")));
        assert_eq!(
            read_entry_bytes(&zip_path, Path::new("sub/ola.txt")).unwrap(),
            "Olá ZIP!".as_bytes()
        );

        // 2. TAR.GZ archive (build minimal valid USTAR header)
        let tgz_path = dir.join("pack.tar.gz");
        {
            let content = b"Conteudo TAR.GZ";
            let mut header = [0u8; 512];
            header[..11].copy_from_slice(b"sub/doc.txt");
            header[100..107].copy_from_slice(b"0000644");
            let sz = format!("{:011o}", content.len());
            header[124..135].copy_from_slice(sz.as_bytes());
            header[136..147].copy_from_slice(b"14600000000");
            header[148..156].copy_from_slice(b"        ");
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let cksum: u32 = header.iter().map(|&b| b as u32).sum();
            let cksum_s = format!("{cksum:06o}\0 ");
            header[148..156].copy_from_slice(cksum_s.as_bytes());

            let f = fs::File::create(&tgz_path).unwrap();
            let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            gz.write_all(&header).unwrap();
            gz.write_all(content).unwrap();
            let pad = 512 - content.len();
            gz.write_all(&vec![0u8; pad]).unwrap();
            gz.write_all(&[0u8; 1024]).unwrap();
            gz.finish().unwrap();
        }
        assert!(is_archive_path(&tgz_path));
        let tmap = scan_source(&tgz_path, &cancel, &mut errs);
        assert!(errs.is_empty(), "tgz errors: {errs:?}");
        assert!(tmap.contains_key(Path::new("sub/doc.txt")));
        assert_eq!(
            read_entry_bytes(&tgz_path, Path::new("sub/doc.txt")).unwrap(),
            b"Conteudo TAR.GZ"
        );

        // 3. Snapshot save & reload
        let snap_path = dir.join("pack.acsnap.json");
        let saved = save_snapshot(&zip_path, &snap_path, &cancel).unwrap();
        assert_eq!(saved, 2);
        let smap = scan_source(&snap_path, &cancel, &mut errs);
        assert!(errs.is_empty(), "snap errors: {errs:?}");
        assert_eq!(
            smap.get(Path::new("sub/ola.txt")).and_then(|m| m.crc32),
            zmap.get(Path::new("sub/ola.txt")).and_then(|m| m.crc32)
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
