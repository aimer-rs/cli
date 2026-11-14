//! Syncing `[[documents]]` into an `AndroidManifest.xml`.
//!
//! Android offers an app a file through an intent filter on its activity: a
//! `VIEW` action for the kinds of data the app opens. This writes those filters,
//! from the project's `[[documents]]`, in a marked block just before the
//! activity's closing tag, so what Aimer writes is rebuilt on every sync and
//! everything outside it stays the user's.
//!
//! Android names a kind of file by MIME type, or, failing that, by the end of its
//! name. An entry with neither `mime_types` nor `extensions` cannot be declared
//! and is reported.

use anyhow::Context;

use super::block;
use super::plist::escape;
use super::scan::{self, TagKind};
use crate::config::DocumentMeta;

/// The outcome of [`sync`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AndroidDocumentsSync {
    pub text: String,
    /// Whether the generated block was written.
    pub wrote: bool,
    /// Whether a block from an earlier sync was removed.
    pub removed_block: bool,
    /// Entries Android cannot declare, by name.
    pub skipped: Vec<String>,
}

/// Make `text` declare `documents` in a generated block before the closing
/// `</activity>`.
///
/// The block of an earlier sync is rebuilt from scratch.
pub(super) fn sync(text: &str, documents: &[DocumentMeta]) -> anyhow::Result<AndroidDocumentsSync> {
    for document in documents {
        if let Some(problem) = document.problem() {
            anyhow::bail!(problem);
        }
    }
    let (stripped, removed_block) = block::DOCUMENTS.strip(text)?;
    let mut sync = AndroidDocumentsSync {
        text: stripped.clone(),
        wrote: false,
        removed_block,
        skipped: Vec::new(),
    };
    let mut lines = Vec::new();
    for document in documents {
        let before = lines.len();
        filters(document, &mut lines);
        if lines.len() == before {
            sync.skipped.push(document.name.clone());
        }
    }
    if lines.is_empty() {
        return Ok(sync);
    }

    let activity_close = scan::tags(&stripped)
        .into_iter()
        .find(|tag| tag.name == "activity" && tag.kind == TagKind::Close)
        .context("the AndroidManifest.xml has no <activity> to declare documents on")?;
    let line_start = stripped[..activity_close.start].rfind('\n').map_or(0, |i| i + 1);
    let leading = &stripped[line_start..activity_close.start];
    let mut text = stripped.clone();
    if leading.trim().is_empty() {
        let indent = format!("{leading}    ");
        text.insert_str(line_start, &block::DOCUMENTS.render(&indent, &lines));
    } else {
        let block = format!("\n{}", block::DOCUMENTS.render("    ", &lines));
        text.insert_str(activity_close.start, &block);
    }
    sync.text = text;
    sync.wrote = true;
    Ok(sync)
}

/// The intent filters for one document: one for its MIME types and one for its
/// extensions, either of which it may not have.
fn filters(document: &DocumentMeta, out: &mut Vec<String>) {
    if !document.mime_types.is_empty() {
        out.push("<intent-filter>".to_string());
        open(out);
        for mime in &document.mime_types {
            out.push(format!("    <data android:mimeType=\"{}\" />", escape_attribute(mime)));
        }
        out.push("</intent-filter>".to_string());
    }
    if !document.extensions.is_empty() {
        out.push("<intent-filter>".to_string());
        open(out);
        // A path pattern is only read next to a host, and a file picked from another
        // app carries no type of its own worth requiring.
        out.push("    <data android:host=\"*\" />".to_string());
        out.push("    <data android:mimeType=\"*/*\" />".to_string());
        for extension in &document.extensions {
            // `\\` in the XML is a single backslash to the matcher: a literal dot.
            out.push(format!("    <data android:pathPattern=\".*\\\\.{}\" />", escape_attribute(extension)));
        }
        out.push("</intent-filter>".to_string());
    }
}

/// The action, category and schemes every filter shares.
fn open(out: &mut Vec<String>) {
    out.push("    <action android:name=\"android.intent.action.VIEW\" />".to_string());
    out.push("    <category android:name=\"android.intent.category.DEFAULT\" />".to_string());
    out.push("    <data android:scheme=\"content\" />".to_string());
    out.push("    <data android:scheme=\"file\" />".to_string());
}

fn escape_attribute(text: &str) -> String {
    escape(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    <application>\n        <activity android:name=\"com.aimer.AimerActivity\">\n            <intent-filter>\n                <action android:name=\"android.intent.action.MAIN\" />\n            </intent-filter>\n        </activity>\n    </application>\n</manifest>\n";

    fn rust() -> DocumentMeta {
        DocumentMeta {
            name: "Rust source file".into(),
            extensions: vec!["rs".into()],
            ..DocumentMeta::default()
        }
    }

    #[test]
    fn an_extension_is_declared_as_a_view_filter_on_the_activity() {
        let out = sync(MANIFEST, &[rust()]).unwrap();

        assert!(out.wrote);
        assert!(out.text.contains(
            "            <!-- aimer:documents:start: generated from [[documents]] in Aimer.toml, edits between the markers are overwritten -->\n            <intent-filter>\n                <action android:name=\"android.intent.action.VIEW\" />\n                <category android:name=\"android.intent.category.DEFAULT\" />\n                <data android:scheme=\"content\" />\n                <data android:scheme=\"file\" />\n                <data android:host=\"*\" />\n                <data android:mimeType=\"*/*\" />\n                <data android:pathPattern=\".*\\\\.rs\" />\n            </intent-filter>\n            <!-- aimer:documents:end -->\n        </activity>"
        ), "{}", out.text);
        assert!(out.text.contains("android.intent.action.MAIN"), "the launcher filter stays");
    }

    #[test]
    fn a_mime_type_is_a_filter_of_its_own() {
        let document = DocumentMeta {
            name: "Markdown".into(),
            extensions: vec!["md".into()],
            mime_types: vec!["text/markdown".into()],
            ..DocumentMeta::default()
        };
        let out = sync(MANIFEST, &[document]).unwrap();

        assert_eq!(out.text.matches("<intent-filter>").count(), 3, "the launcher's, one for the type, one for the extension");
        assert!(out.text.contains("<data android:mimeType=\"text/markdown\" />"));
        assert!(out.text.contains("<data android:pathPattern=\".*\\\\.md\" />"));
    }

    #[test]
    fn an_entry_with_only_a_type_identifier_is_skipped_and_reported() {
        let ios_only = DocumentMeta {
            name: "Code".into(),
            utis: vec!["public.source-code".into()],
            ..DocumentMeta::default()
        };
        let out = sync(MANIFEST, &[ios_only, rust()]).unwrap();

        assert_eq!(out.skipped, ["Code"]);
        assert!(out.wrote, "the other entry is still written");
        assert!(!out.text.contains("public.source-code"));
    }

    #[test]
    fn a_second_sync_changes_nothing_and_dropping_everything_restores_the_file() {
        let first = sync(MANIFEST, &[rust()]).unwrap();
        let second = sync(&first.text, &[rust()]).unwrap();
        let gone = sync(&first.text, &[]).unwrap();

        assert_eq!(second.text, first.text);
        assert_eq!(gone.text, MANIFEST);
        assert!(gone.removed_block && !gone.wrote);
    }

    #[test]
    fn a_manifest_with_no_activity_is_an_error_not_a_silent_skip() {
        let error = sync("<manifest><application/></manifest>", &[rust()]).unwrap_err().to_string();

        assert!(error.contains("no <activity>"), "{error}");
    }

    #[test]
    fn a_bad_extension_is_refused() {
        let bad = DocumentMeta { name: "Rust".into(), extensions: vec![".rs".into()], ..DocumentMeta::default() };

        assert!(sync(MANIFEST, &[bad]).is_err());
    }
}
