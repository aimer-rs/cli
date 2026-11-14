//! Syncing `[[documents]]` into an `Info.plist`.
//!
//! The system offers an app only the files it declares it can open. This writes
//! that declaration, `CFBundleDocumentTypes`, from the project's `[[documents]]`,
//! in the same kind of marked block the permissions use, so what Aimer writes is
//! rebuilt on every sync and everything outside it stays the user's.
//!
//! macOS names a kind of file by extension or type identifier; iOS only by type
//! identifier, so an entry with no `utis` is not written for iOS and is reported.

use anyhow::Context;

use super::block;
use super::plist::{self, escape};
use crate::config::{DocumentMeta, DocumentRole};

/// The key that declares the kinds of file an app opens.
const KEY: &str = "CFBundleDocumentTypes";

/// Which `Info.plist` is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Apple {
    Macos,
    Ios,
}

/// The outcome of [`sync`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DocumentsSync {
    pub text: String,
    /// Whether the generated block was written.
    pub wrote: bool,
    /// Whether the file declares `CFBundleDocumentTypes` itself, so nothing was
    /// written: a second declaration would be a duplicate key, and the user's wins.
    pub kept_own: bool,
    /// Whether a block from an earlier sync was removed.
    pub removed_block: bool,
    /// Entries this platform cannot declare, by name: iOS needs a type identifier.
    pub skipped: Vec<String>,
}

/// Make `text` declare `documents` in a generated block before the closing
/// `</dict>`.
///
/// The block of an earlier sync is rebuilt from scratch. A file that declares the
/// key itself is left alone.
pub(super) fn sync(
    text: &str,
    documents: &[DocumentMeta],
    platform: Apple,
) -> anyhow::Result<DocumentsSync> {
    for document in documents {
        if let Some(problem) = document.problem() {
            anyhow::bail!(problem);
        }
    }
    let (stripped, removed_block) = block::DOCUMENTS.strip(text)?;
    let mut sync = DocumentsSync {
        text: stripped.clone(),
        wrote: false,
        kept_own: false,
        removed_block,
        skipped: Vec::new(),
    };
    let (declarable, skipped): (Vec<&DocumentMeta>, Vec<&DocumentMeta>) = documents
        .iter()
        .partition(|document| platform == Apple::Macos || !document.utis.is_empty());
    sync.skipped = skipped.iter().map(|document| document.name.clone()).collect();
    if declarable.is_empty() {
        return Ok(sync);
    }

    let (existing, dict_close) = plist::top_level(&stripped).context("reading the Info.plist")?;
    if existing.iter().any(|entry| entry.key == KEY) {
        sync.kept_own = true;
        return Ok(sync);
    }
    let mut text = stripped;
    plist::insert_marked_block(&mut text, dict_close, block::DOCUMENTS, &lines(&declarable, platform));
    sync.text = text;
    sync.wrote = true;
    Ok(sync)
}

/// The `<key>` and `<array>` that declare `documents`, one line each.
fn lines(documents: &[&DocumentMeta], platform: Apple) -> Vec<String> {
    let mut out = vec![format!("<key>{KEY}</key>"), "<array>".to_string()];
    for document in documents {
        out.push("    <dict>".to_string());
        push_string(&mut out, "CFBundleTypeName", &document.name);
        push_string(
            &mut out,
            "CFBundleTypeRole",
            match document.role {
                DocumentRole::Editor => "Editor",
                DocumentRole::Viewer => "Viewer",
            },
        );
        // iOS does not read extensions; a kind of file is its type identifiers.
        if platform == Apple::Macos {
            push_array(&mut out, "CFBundleTypeExtensions", &document.extensions);
        }
        push_array(&mut out, "LSItemContentTypes", &document.utis);
        // Offered in "Open With" and accepted on the icon, without taking a kind of
        // file over from the app the user already opens it with.
        push_string(&mut out, "LSHandlerRank", "Alternate");
        out.push("    </dict>".to_string());
    }
    out.push("</array>".to_string());
    out
}

fn push_string(out: &mut Vec<String>, key: &str, value: &str) {
    out.push(format!("        <key>{key}</key>"));
    out.push(format!("        <string>{}</string>", escape(value)));
}

/// An `<array>` of strings under `key`; nothing at all for an empty list, which
/// the system reads as "any" rather than "none".
fn push_array(out: &mut Vec<String>, key: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    out.push(format!("        <key>{key}</key>"));
    out.push("        <array>".to_string());
    for value in values {
        out.push(format!("            <string>{}</string>", escape(value)));
    }
    out.push("        </array>".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLIST: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n    <key>CFBundleName</key>\n    <string>App</string>\n</dict>\n</plist>\n";

    fn rust() -> DocumentMeta {
        DocumentMeta {
            name: "Rust source file".into(),
            extensions: vec!["rs".into()],
            ..DocumentMeta::default()
        }
    }

    #[test]
    fn a_document_is_declared_in_a_marked_block_before_the_closing_dict() {
        let out = sync(PLIST, &[rust()], Apple::Macos).unwrap();

        assert!(out.wrote && !out.kept_own);
        assert!(out.text.contains(
            "    <!-- aimer:documents:start: generated from [[documents]] in Aimer.toml, edits between the markers are overwritten -->\n    <key>CFBundleDocumentTypes</key>\n    <array>\n        <dict>\n            <key>CFBundleTypeName</key>\n            <string>Rust source file</string>\n            <key>CFBundleTypeRole</key>\n            <string>Editor</string>\n            <key>CFBundleTypeExtensions</key>\n            <array>\n                <string>rs</string>\n            </array>\n            <key>LSHandlerRank</key>\n            <string>Alternate</string>\n        </dict>\n    </array>\n    <!-- aimer:documents:end -->\n</dict>"
        ), "{}", out.text);
        assert!(out.text.contains("<key>CFBundleName</key>"), "the user's entries stay");
    }

    #[test]
    fn a_second_sync_changes_nothing() {
        let first = sync(PLIST, &[rust()], Apple::Macos).unwrap();
        let second = sync(&first.text, &[rust()], Apple::Macos).unwrap();

        assert_eq!(second.text, first.text);
        assert!(second.removed_block && second.wrote, "rebuilt from scratch, to the same text");
    }

    #[test]
    fn removing_the_documents_removes_the_block_and_leaves_no_trace() {
        let first = sync(PLIST, &[rust()], Apple::Macos).unwrap();
        let gone = sync(&first.text, &[], Apple::Macos).unwrap();

        assert_eq!(gone.text, PLIST);
        assert!(gone.removed_block && !gone.wrote);
    }

    #[test]
    fn types_viewers_and_text_that_needs_escaping_are_written_faithfully() {
        let document = DocumentMeta {
            name: "Plans & <sketches>".into(),
            extensions: vec!["plan".into(), "sk".into()],
            utis: vec!["public.data".into()],
            role: DocumentRole::Viewer,
            ..DocumentMeta::default()
        };
        let text = sync(PLIST, &[document], Apple::Macos).unwrap().text;

        assert!(text.contains("<string>Plans &amp; &lt;sketches&gt;</string>"), "{text}");
        assert!(text.contains("<string>Viewer</string>"));
        assert!(text.contains("<string>plan</string>") && text.contains("<string>sk</string>"));
        assert!(text.contains("<key>LSItemContentTypes</key>") && text.contains("<string>public.data</string>"));
    }

    #[test]
    fn a_kind_of_file_given_only_by_type_has_no_extension_list() {
        let document = DocumentMeta {
            name: "Code".into(),
            utis: vec!["public.source-code".into()],
            ..DocumentMeta::default()
        };
        let text = sync(PLIST, &[document], Apple::Macos).unwrap().text;

        assert!(!text.contains("CFBundleTypeExtensions"), "an empty list would mean any extension");
    }

    #[test]
    fn a_declaration_the_user_wrote_is_left_alone() {
        let own = PLIST.replace(
            "</dict>\n</plist>",
            "    <key>CFBundleDocumentTypes</key>\n    <array/>\n</dict>\n</plist>",
        );
        let out = sync(&own, &[rust()], Apple::Macos).unwrap();

        assert!(out.kept_own && !out.wrote);
        assert_eq!(out.text, own);
    }

    #[test]
    fn an_entry_that_cannot_be_declared_is_an_error_not_a_silent_skip() {
        let bad = DocumentMeta {
            name: "Rust".into(),
            extensions: vec![".rs".into()],
            ..DocumentMeta::default()
        };

        let error = sync(PLIST, &[bad], Apple::Macos).unwrap_err().to_string();

        assert!(error.contains("not an extension"), "{error}");
    }

    #[test]
    fn ios_declares_only_the_kinds_of_file_that_have_a_type_identifier() {
        let typed = DocumentMeta {
            name: "Plain text".into(),
            extensions: vec!["txt".into()],
            utis: vec!["public.plain-text".into()],
            ..DocumentMeta::default()
        };
        let out = sync(PLIST, &[typed, rust()], Apple::Ios).unwrap();

        assert!(out.wrote);
        assert_eq!(out.skipped, ["Rust source file"], "an extension alone is not a kind of file on iOS");
        assert!(out.text.contains("<string>public.plain-text</string>"));
        assert!(!out.text.contains("CFBundleTypeExtensions"), "iOS does not read them");
        assert!(!out.text.contains("Rust source file"));
    }

    #[test]
    fn ios_with_nothing_declarable_writes_no_block_and_says_what_it_skipped() {
        let out = sync(PLIST, &[rust()], Apple::Ios).unwrap();

        assert!(!out.wrote);
        assert_eq!(out.skipped, ["Rust source file"]);
        assert_eq!(out.text, PLIST);
    }
}
