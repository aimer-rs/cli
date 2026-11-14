//! The `[[documents]]` array of `Aimer.toml`: the kinds of file the app opens.

use serde::{Deserialize, Serialize};

/// How the app treats a kind of file, which decides where the system offers it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentRole {
    /// The app reads and changes the file: a text editor opening source code.
    #[default]
    Editor,
    /// The app only shows the file: an image viewer.
    Viewer,
}

/// One kind of file the app can open, declared as a `[[documents]]` entry.
///
/// ```toml
/// [[documents]]
/// name = "Rust source file"
/// extensions = ["rs"]
/// role = "editor"
/// ```
///
/// With it declared, a file of that kind dropped on the app's icon, or chosen
/// with "Open With", reaches the app as a file-open request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// What the system calls this kind of file, such as "Rust source file".
    pub name: String,
    /// File name extensions without the dot: `["rs"]`.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Uniform type identifiers (`public.source-code`). Apple's systems identify
    /// kinds of file by these, and iOS has no other way to; they are optional on
    /// macOS, where the extensions are enough.
    #[serde(default)]
    pub utis: Vec<String>,
    /// MIME types (`text/x-rust`), which is how Android names a kind of file.
    #[serde(default)]
    pub mime_types: Vec<String>,
    /// Whether the app edits these files or only views them.
    #[serde(default)]
    pub role: DocumentRole,
}

impl DocumentMeta {
    /// Why this entry cannot be written into a platform file, if it cannot.
    ///
    /// An entry that names no kind of file, or one whose extension is not one
    /// (a dot, a space, a path), would be a declaration the system silently
    /// ignores or misreads, so it is refused where the mistake was made.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("a [[documents]] entry needs a `name`".to_string());
        }
        if self.extensions.is_empty() && self.utis.is_empty() && self.mime_types.is_empty() {
            return Some(format!(
                "[[documents]] \"{}\" names no kind of file: give `extensions`, `utis` or `mime_types`",
                self.name
            ));
        }
        for extension in &self.extensions {
            let valid = !extension.is_empty()
                && extension
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'));
            if !valid {
                return Some(format!(
                    "[[documents]] \"{}\": \"{extension}\" is not an extension; write it without the dot, such as \"rs\"",
                    self.name
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AimerManifest;

    fn parse(documents: &str) -> AimerManifest {
        toml::from_str(&format!("[package]\nname = \"App\"\n{documents}")).unwrap()
    }

    #[test]
    fn documents_are_read_from_the_manifest_as_an_array_of_tables() {
        let manifest = parse(
            r#"
            [[documents]]
            name = "Rust source file"
            extensions = ["rs"]
            utis = ["public.source-code"]

            [[documents]]
            name = "Image"
            extensions = ["png", "jpg"]
            role = "viewer"
            "#,
        );

        assert_eq!(manifest.documents.len(), 2);
        assert_eq!(manifest.documents[0].name, "Rust source file");
        assert_eq!(manifest.documents[0].role, DocumentRole::Editor, "editor is the default");
        assert_eq!(manifest.documents[0].utis, ["public.source-code"]);
        assert_eq!(manifest.documents[1].extensions, ["png", "jpg"]);
        assert_eq!(manifest.documents[1].role, DocumentRole::Viewer);
    }

    #[test]
    fn a_manifest_without_documents_has_none() {
        assert!(parse("").documents.is_empty());
    }

    #[test]
    fn an_entry_that_names_no_file_kind_or_a_bad_extension_is_a_problem() {
        let doc = |name: &str, extensions: &[&str]| DocumentMeta {
            name: name.into(),
            extensions: extensions.iter().map(ToString::to_string).collect(),
            ..DocumentMeta::default()
        };
        assert_eq!(doc("Rust", &["rs"]).problem(), None);
        assert!(doc("Tar", &["tar.gz"]).problem().is_some(), "a dot inside is refused");
        assert!(doc("Rust", &[".rs"]).problem().is_some(), "the dot is not part of it");
        assert!(doc("Rust", &["r s"]).problem().is_some());
        assert!(doc("Rust", &[""]).problem().is_some());
        assert!(doc("", &["rs"]).problem().is_some(), "a name is required");
        assert!(doc("Nothing", &[]).problem().is_some(), "an entry must name some kind of file");
        let by_type = DocumentMeta { name: "Code".into(), utis: vec!["public.source-code".into()], ..DocumentMeta::default() };
        assert_eq!(by_type.problem(), None, "a type identifier alone is a kind of file");
    }
}
