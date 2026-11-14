//! The block of generated entries between two marker comments.
//!
//! Everything Aimer writes lives between the markers and is rebuilt from
//! `Aimer.toml` on every sync. Everything outside them belongs to the user.

use anyhow::bail;

/// Which generated block: the markers that bound it and the `Aimer.toml` table it
/// comes from, so one file can hold more than one without them touching.
#[derive(Clone, Copy)]
pub(super) struct Marker {
    name: &'static str,
    source: &'static str,
}

/// The block built from `[permissions]`.
pub(super) const PERMISSIONS: Marker = Marker {
    name: "permissions",
    source: "[permissions]",
};

/// The block built from `[[documents]]`.
pub(super) const DOCUMENTS: Marker = Marker {
    name: "documents",
    source: "[[documents]]",
};

impl Marker {
    fn start(self) -> String {
        format!("<!-- aimer:{}:start", self.name)
    }

    fn end(self) -> String {
        format!("<!-- aimer:{}:end", self.name)
    }

    /// The comment that opens the generated block.
    pub(super) fn start_comment(self) -> String {
        format!(
            "{}: generated from {} in Aimer.toml, edits between the markers are overwritten -->",
            self.start(),
            self.source
        )
    }

    /// The comment that closes the generated block.
    pub(super) fn end_comment(self) -> String {
        format!("{} -->", self.end())
    }

    /// `lines` between the two markers, one per line, each indented by `indent`
    /// and ending in a newline.
    pub(super) fn render(self, indent: &str, lines: &[String]) -> String {
        let mut block = String::new();
        for line in std::iter::once(self.start_comment())
            .chain(lines.iter().cloned())
            .chain(std::iter::once(self.end_comment()))
        {
            block.push_str(indent);
            block.push_str(&line);
            block.push('\n');
        }
        block
    }

    /// `text` without its generated block, and whether there was one.
    ///
    /// Whole lines are removed, so a block added by [`Marker::render`] leaves no
    /// trace.
    pub(super) fn strip(self, text: &str) -> anyhow::Result<(String, bool)> {
        let (start_marker, end_marker) = (self.start(), self.end());
        let Some(start) = text.find(&start_marker) else {
            if text.contains(&end_marker) {
                bail!("found `{end_marker}` without `{start_marker}`; remove the stray marker or restore the block");
            }
            return Ok((text.to_string(), false));
        };
        let Some(end_at) = text[start..].find(&end_marker).map(|i| start + i) else {
            bail!("found `{start_marker}` without `{end_marker}`; restore the end marker or delete the block");
        };
        let end = text[end_at..]
            .find("-->")
            .map_or(text.len(), |i| end_at + i + 3);

        let line_start = match text[..start].rfind('\n') {
            Some(i) if text[i + 1..start].trim().is_empty() => i + 1,
            None if text[..start].trim().is_empty() => 0,
            _ => start,
        };
        let line_end = match text[end..].find('\n') {
            Some(i) if text[end..end + i].trim().is_empty() => end + i + 1,
            None if text[end..].trim().is_empty() => text.len(),
            _ => end,
        };
        Ok((
            format!("{}{}", &text[..line_start], &text[line_end..]),
            true,
        ))
    }
}

/// The comment that opens the `[permissions]` block.
pub(super) fn start_comment() -> String {
    PERMISSIONS.start_comment()
}

/// The comment that closes the `[permissions]` block.
pub(super) fn end_comment() -> String {
    PERMISSIONS.end_comment()
}

/// The `[permissions]` block's `lines`, rendered.
pub(super) fn render(indent: &str, lines: &[String]) -> String {
    PERMISSIONS.render(indent, lines)
}

/// `text` without its `[permissions]` block, and whether there was one.
pub(super) fn strip(text: &str) -> anyhow::Result<(String, bool)> {
    PERMISSIONS.strip(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_block_is_removed_without_a_trace() {
        let block = render("    ", &["<a/>".to_string()]);
        let text = format!("<x>\n{block}</x>\n");

        let (stripped, found) = strip(&text).unwrap();

        assert!(found);
        assert_eq!(stripped, "<x>\n</x>\n");
    }

    #[test]
    fn text_without_a_block_is_returned_as_is() {
        let (stripped, found) = strip("<x/>\n").unwrap();

        assert!(!found);
        assert_eq!(stripped, "<x/>\n");
    }

    #[test]
    fn a_lone_marker_is_an_error_instead_of_a_guess() {
        let lone_start = format!("<x>\n{}\n</x>", start_comment());
        let lone_end = format!("<x>\n{}\n</x>", end_comment());

        assert!(
            strip(&lone_start)
                .unwrap_err()
                .to_string()
                .contains("without")
        );
        assert!(
            strip(&lone_end)
                .unwrap_err()
                .to_string()
                .contains("without")
        );
    }

    #[test]
    fn the_documents_block_and_the_permissions_block_do_not_touch_each_other() {
        let permissions = render("  ", &["<p/>".to_string()]);
        let documents = DOCUMENTS.render("  ", &["<d/>".to_string()]);
        let text = format!("<x>\n{permissions}{documents}</x>\n");

        let (without_documents, found) = DOCUMENTS.strip(&text).unwrap();
        assert!(found);
        assert_eq!(without_documents, format!("<x>\n{permissions}</x>\n"));

        let (without_permissions, found) = strip(&text).unwrap();
        assert!(found);
        assert_eq!(without_permissions, format!("<x>\n{documents}</x>\n"));
        assert!(documents.contains("generated from [[documents]] in Aimer.toml"));
    }
}
