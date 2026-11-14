//! Syncing the `[permissions]` of `Aimer.toml` into the platform files.
//!
//! `aimer assemble` (and `aimer run` / `aimer package`, which share its steps)
//! translates the portable table into `Info.plist` keys and Android
//! `<uses-permission>` elements, so the permissions are declared once.
//!
//! `Info.plist` and `AndroidManifest.xml` stay user-owned files. What Aimer
//! writes sits between `aimer:permissions:start` and `aimer:permissions:end`
//! comments and is rebuilt on every sync; everything outside is the user's. A
//! hand-written `Info.plist` entry that `Aimer.toml` replaces is commented out,
//! never deleted, and reported.

mod android;
mod block;
mod documents;
mod documents_android;
mod plist;
mod scan;
#[cfg(test)]
mod tests;

use std::path::Path;

use anyhow::Context;

use crate::commands::assemble::Reporter;
use crate::config::{AimerManifest, AndroidUse};

/// The Android manifest, relative to the project root.
const ANDROID_MANIFEST: &str = "builds/android/app/src/main/AndroidManifest.xml";

/// Sync `[permissions]` and `[macos.permissions]` into the macOS `Info.plist`.
pub(crate) fn sync_macos(pkg_name: &str, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let extra = manifest.macos.map(|m| m.permissions).unwrap_or_default();
    let entries = manifest.permissions.apple_entries(&extra);
    let path = format!("builds/macos/{pkg_name}/Info.plist");
    sync_plist_file(Path::new(&path), &entries, reporter)
}

/// Sync `[[documents]]` into the macOS `Info.plist`: the kinds of file the app
/// opens, which is what makes the system offer it a file dropped on its icon.
pub(crate) fn sync_documents_macos(pkg_name: &str, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let path = format!("builds/macos/{pkg_name}/Info.plist");
    sync_documents_file(Path::new(&path), &manifest.documents, documents::Apple::Macos, reporter)
}

/// Sync `[[documents]]` into the iOS `Info.plist`. iOS identifies a kind of file
/// by type identifier only, so an entry with no `utis` is skipped with a note.
pub(crate) fn sync_documents_ios(pkg_name: &str, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let path = format!("builds/ios/{pkg_name}/Info.plist");
    sync_documents_file(Path::new(&path), &manifest.documents, documents::Apple::Ios, reporter)
}

fn sync_documents_file(
    path: &Path,
    documents: &[crate::config::DocumentMeta],
    platform: documents::Apple,
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    let Some(text) = read(path, !documents.is_empty(), reporter)? else {
        return Ok(());
    };
    let out = documents::sync(&text, documents, platform)
        .with_context(|| format!("syncing {}", path.display()))?;

    let shown = path.display();
    for name in &out.skipped {
        reporter.note(format!(
            "Documents: [[documents]] \"{name}\" is not written into {shown}: iOS names a kind of file by type identifier, so give it `utis`"
        ));
    }
    if out.kept_own {
        reporter.note(format!(
            "Documents: {shown} already declares CFBundleDocumentTypes, so [[documents]] was not written; remove that entry to let Aimer.toml declare it"
        ));
    }
    if out.wrote {
        reporter.note(format!(
            "Documents: wrote {} into {shown}",
            documents
                .iter()
                .filter(|d| !out.skipped.contains(&d.name))
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if documents.is_empty() && out.removed_block {
        reporter.note(format!(
            "Documents: removed the generated block from {shown}"
        ));
    }
    write_if_changed(path, &text, &out.text)
}

/// Sync `[[documents]]` into the Android manifest as intent filters on the
/// activity, which is what makes Android offer the app a file.
pub(crate) fn sync_documents_android(reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let path = Path::new(ANDROID_MANIFEST);
    let documents = &manifest.documents;
    let Some(text) = read(path, !documents.is_empty(), reporter)? else {
        return Ok(());
    };
    let out = documents_android::sync(&text, documents)
        .with_context(|| format!("syncing {}", path.display()))?;

    let shown = path.display();
    for name in &out.skipped {
        reporter.note(format!(
            "Documents: [[documents]] \"{name}\" is not written into {shown}: Android needs `mime_types` or `extensions`"
        ));
    }
    if out.wrote {
        reporter.note(format!("Documents: wrote intent filters into {shown}"));
    }
    if documents.is_empty() && out.removed_block {
        reporter.note(format!("Documents: removed the generated block from {shown}"));
    }
    write_if_changed(path, &text, &out.text)
}

/// Sync `[permissions]` and `[ios.permissions]` into the iOS `Info.plist`.
pub(crate) fn sync_ios(pkg_name: &str, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let extra = manifest.ios.map(|m| m.permissions).unwrap_or_default();
    let entries = manifest.permissions.apple_entries(&extra);
    let path = format!("builds/ios/{pkg_name}/Info.plist");
    sync_plist_file(Path::new(&path), &entries, reporter)
}

/// Sync `[permissions]` and `[android.permissions]` into the Android manifest.
pub(crate) fn sync_android(reporter: &dyn Reporter) -> anyhow::Result<()> {
    let Some(manifest) = AimerManifest::load()? else {
        return Ok(());
    };
    let extra = manifest.android.and_then(|m| m.permissions);
    let uses = manifest.permissions.android_uses(extra.as_ref());
    sync_android_file(Path::new(ANDROID_MANIFEST), &uses, reporter)
}

fn sync_plist_file(
    path: &Path,
    entries: &[(String, String)],
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    let Some(text) = read(path, !entries.is_empty(), reporter)? else {
        return Ok(());
    };
    let out = plist::sync(&text, entries).with_context(|| format!("syncing {}", path.display()))?;

    let shown = path.display();
    for item in &out.overridden {
        reporter.note(format!(
            "Permissions: {shown}: {} was \"{}\", Aimer.toml sets \"{}\"; the previous entry is kept as a comment",
            item.key, item.previous, item.new
        ));
    }
    if !out.added.is_empty() {
        reporter.note(format!(
            "Permissions: wrote {} into {shown}",
            out.added.join(", ")
        ));
    }
    if entries.is_empty() && out.removed_block {
        reporter.note(format!(
            "Permissions: removed the generated block from {shown}"
        ));
    }
    write_if_changed(path, &text, &out.text)
}

fn sync_android_file(
    path: &Path,
    uses: &[AndroidUse],
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    let Some(text) = read(path, !uses.is_empty(), reporter)? else {
        return Ok(());
    };
    let out = android::sync(&text, uses).with_context(|| format!("syncing {}", path.display()))?;

    let shown = path.display();
    if !out.added.is_empty() {
        reporter.note(format!(
            "Permissions: wrote {} into {shown}",
            out.added.join(", ")
        ));
    }
    if uses.is_empty() && out.removed_block {
        reporter.note(format!(
            "Permissions: removed the generated block from {shown}"
        ));
    }
    write_if_changed(path, &text, &out.text)
}

/// The text of `path`, or `None` when there is no such file. A missing file is
/// only worth a warning when permissions are declared.
fn read(path: &Path, declared: bool, reporter: &dyn Reporter) -> anyhow::Result<Option<String>> {
    if !path.exists() {
        if declared {
            reporter.note(format!(
                "warning: {} not found; skipping its permissions",
                path.display()
            ));
        }
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("reading {}", path.display()))
}

/// Leave the file (and its modification time) alone when nothing changed.
fn write_if_changed(path: &Path, old: &str, new: &str) -> anyhow::Result<()> {
    if old != new {
        std::fs::write(path, new).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}
