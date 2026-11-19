//! Include/theme file resolution helpers.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use url::Url;
use zip::ZipArchive;

use super::IncludeMode;
use crate::Result;

pub(super) fn split_include_target(target: &str) -> (&str, Option<&str>) {
    let target = target.trim();
    if target.starts_with('<') {
        if let Some(end) = target.find('>') {
            let path = &target[..=end];
            if target.as_bytes().get(end + 1) == Some(&b'!') {
                return (path, Some(&target[end + 2..]));
            }
            return (path, None);
        }
        return (target, None);
    }

    if let Some((path, selector)) = target.rsplit_once('!') {
        if !selector.trim().is_empty() {
            return (path.trim(), Some(selector.trim()));
        }
    }

    (target, None)
}

pub(super) fn extract_include_target(trimmed: &str) -> Option<&str> {
    for prefix in [
        "!include_many ",
        "!include_once ",
        "!includesub ",
        "!include ",
        "!includeurl ",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }
    None
}

pub(super) fn build_include_key(path: &str, selector: Option<&str>, mode: IncludeMode) -> String {
    let mode_name = match mode {
        IncludeMode::Include => "include",
        IncludeMode::IncludeOnce => "include_once",
        IncludeMode::IncludeSub => "includesub",
        IncludeMode::Many => "include_many",
    };
    match selector {
        Some(selector) => format!("{mode_name}:{path}!{selector}"),
        None => format!("{mode_name}:{path}"),
    }
}

pub(super) fn is_remote_reference(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

pub(super) fn parent_url(url: &Url) -> Url {
    let mut parent = url.clone();
    if let Ok(mut segments) = parent.path_segments_mut() {
        segments.pop_if_empty();
        segments.pop();
        segments.push("");
    }
    parent
}

pub(super) fn default_remote_theme_url(theme_file: &str) -> String {
    format!("https://raw.githubusercontent.com/bschwarz/puml-themes/master/themes/{theme_file}")
}

pub(super) fn extract_subpart_source(source: &str, selector: &str) -> Result<String> {
    let mut result = Vec::new();
    let mut in_match = false;
    let mut found = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = super::strip_directive_prefix(trimmed, "!startsub ") {
            in_match = rest.trim() == selector;
            found |= in_match;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("!endsub") {
            in_match = false;
            continue;
        }
        if in_match {
            result.push(line.to_string());
        }
    }

    if found {
        Ok(result.join("\n"))
    } else {
        Err(crate::Error::Parse {
            line: 1,
            column: Some(1),
            message: format!("included subpart not found: {selector}"),
        })
    }
}

pub(super) fn extract_diagram_source(source: &str, selector: &str) -> Result<String> {
    let mut blocks: Vec<(Option<String>, Vec<String>)> = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body: Vec<String> = Vec::new();
    let mut in_block = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("@startuml") {
            in_block = true;
            current_id = parse_startuml_id(trimmed);
            current_body.clear();
            continue;
        }
        if trimmed.starts_with("@enduml") {
            if in_block {
                blocks.push((current_id.clone(), current_body.clone()));
            }
            in_block = false;
            current_id = None;
            current_body.clear();
            continue;
        }
        if in_block {
            current_body.push(line.to_string());
        }
    }

    if blocks.is_empty() {
        return Err(crate::Error::Parse {
            line: 1,
            column: Some(1),
            message: format!("included diagram block not found: {selector}"),
        });
    }

    if let Ok(index) = selector.parse::<usize>() {
        if let Some((_, body)) = blocks.get(index) {
            return Ok(body.join("\n"));
        }
    } else if let Some((_, body)) = blocks
        .iter()
        .find(|(id, _)| id.as_deref() == Some(selector))
    {
        return Ok(body.join("\n"));
    }

    Err(crate::Error::Parse {
        line: 1,
        column: Some(1),
        message: format!("included diagram block not found: {selector}"),
    })
}

pub(super) fn parse_startuml_id(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let close = line[open + 1..].find(')')? + open + 1;
    let inner = line[open + 1..close].trim();
    let (_, value) = inner.split_once("id=")?;
    let id = value.trim().trim_matches('"');
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub(super) fn theme_filename(theme_name: &str) -> String {
    let theme_name = theme_name.trim().trim_matches('"');
    if theme_name.starts_with("puml-theme-") && theme_name.ends_with(".puml") {
        theme_name.to_string()
    } else if theme_name.starts_with("puml-theme-") {
        format!("{theme_name}.puml")
    } else if theme_name.ends_with(".puml") {
        theme_name.to_string()
    } else {
        format!("puml-theme-{theme_name}.puml")
    }
}

pub(super) fn split_keyword_from(s: &str) -> Option<(&str, &str)> {
    let lower = s.to_ascii_lowercase();
    let idx = lower.find(" from ")?;
    Some((&s[..idx], &s[idx + " from ".len()..]))
}

pub(super) fn normalize_import_entry_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg_attr(not(feature = "remote"), allow(dead_code))]
pub(super) fn make_remote_temp_path(url: &str) -> PathBuf {
    let parsed = Url::parse(url).ok();
    let stem = parsed
        .as_ref()
        .and_then(url::Url::path_segments)
        .and_then(|mut segments| segments.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("remote.puml");

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();

    std::env::temp_dir()
        .join("plantuml-little-remote")
        .join(format!("{hash:016x}-{stem}"))
}

pub(super) fn extract_archive_to_temp(archive_path: &Path) -> Result<PathBuf> {
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(io::Error::other)?;
    let temp_root = make_archive_temp_dir(archive_path);
    fs::create_dir_all(&temp_root)?;

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).map_err(io::Error::other)?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let out_path = temp_root.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&out_path)?;
        io::copy(&mut entry, &mut out)?;
    }

    Ok(temp_root)
}

pub(super) fn make_archive_temp_dir(archive_path: &Path) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    let stem = archive_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("import");
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "plantuml-little-import-{stem}-{}-{suffix}",
        std::process::id()
    ))
}
