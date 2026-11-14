//! Which tools the server lists, given what the connection can currently use.

use std::collections::HashSet;
use std::sync::OnceLock;

use rmcp::model::Tool;

use super::AimerMcpServer;

/// What the connection can use right now, which decides the tools it is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct ToolAvailability {
    /// A running app takes input, so the tools that drive it can act.
    pub app_connected: bool,
    /// The embedded rust-analyzer engine has been started, so the language tools can answer.
    pub lsp_started: bool,
}

/// The tools that need the embedded rust-analyzer engine. `start_lsp` is not one of
/// them, because it is how the engine gets started, and neither are the tools that
/// only read files, run Cargo, or apply an already reviewed edit.
const LSP_TOOL_NAMES: &[&str] = &[
    "search_symbols",
    "get_document_symbols",
    "completion",
    "get_code_lenses",
    "find_definition",
    "find_declaration",
    "find_references",
    "find_implementations",
    "type_definition",
    "hover",
    "document_highlights",
    "signature_help",
    "prepare_rename",
    "inlay_hints",
    "get_diagnostics",
    "get_call_hierarchy",
    "expand_macro",
    "get_code_actions",
    "rename_symbol",
    "preview_code_action",
    "format",
];

/// The names of the tools that act on the running app, which are exactly the
/// input tools.
pub(super) fn app_tool_names() -> &'static HashSet<String> {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        AimerMcpServer::input_tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    })
}

/// `tools` without the ones that drive the app unless one is connected, and without
/// the language tools until the engine has been started.
pub(super) fn offered_tools(mut tools: Vec<Tool>, availability: ToolAvailability) -> Vec<Tool> {
    tools.retain(|tool| {
        let name = tool.name.as_ref();
        (availability.app_connected || !app_tool_names().contains(name))
            && (availability.lsp_started || !LSP_TOOL_NAMES.contains(&name))
    });
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(app_connected: bool, lsp_started: bool) -> Vec<String> {
        offered_tools(
            AimerMcpServer::tool_router().list_all(),
            ToolAvailability {
                app_connected,
                lsp_started,
            },
        )
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
    }

    fn all_names() -> HashSet<String> {
        AimerMcpServer::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }

    #[test]
    fn without_a_connected_app_the_tools_that_drive_it_are_not_listed() {
        let names = offered(false, true);
        for tool in [
            "left_click",
            "right_click",
            "double_click",
            "mouse_move",
            "scroll",
            "key",
            "type",
            "tap",
            "swipe",
            "pinch",
            "send_input",
            "screenshot",
        ] {
            assert!(!names.iter().any(|n| n == tool), "{tool} needs an app");
        }
    }

    #[test]
    fn the_tools_that_need_neither_an_app_nor_the_engine_are_always_listed() {
        // Session control, Cargo and file tools work on a session that is still
        // building its app, and on none at all.
        for connected in [false, true] {
            for started in [false, true] {
                let names = offered(connected, started);
                for tool in [
                    "restart_project",
                    "stop_project",
                    "get_logs",
                    "cargo_check",
                    "search_text",
                    "inspect_file",
                    "preview_edit",
                    "apply_edit",
                    "start_lsp",
                    "list_sessions",
                    "attach_session",
                    "detach_session",
                ] {
                    assert!(
                        names.iter().any(|n| n == tool),
                        "{tool} (app: {connected}, lsp: {started})"
                    );
                }
            }
        }
    }

    #[test]
    fn the_language_tools_are_listed_only_once_the_engine_has_started() {
        for connected in [false, true] {
            let before = offered(connected, false);
            let after = offered(connected, true);
            for tool in LSP_TOOL_NAMES {
                assert!(!before.iter().any(|n| n == tool), "{tool} before start");
                assert!(after.iter().any(|n| n == tool), "{tool} after start");
            }
        }
    }

    #[test]
    fn every_language_tool_name_is_a_real_tool() {
        let all = all_names();
        for tool in LSP_TOOL_NAMES {
            assert!(all.contains(*tool), "{tool} is not a registered tool");
        }
        assert!(!LSP_TOOL_NAMES.contains(&"start_lsp"));
    }

    #[test]
    fn with_a_connected_app_and_a_started_engine_every_tool_is_listed() {
        let all = all_names();
        assert_eq!(offered(true, true).len(), all.len());
        assert!(offered(false, true).len() < all.len());
        assert!(offered(true, false).len() < all.len());
        assert!(offered(true, true).iter().any(|n| n == "left_click"));
    }

    #[test]
    fn the_app_tools_are_exactly_the_input_tools() {
        let input: HashSet<String> = AimerMcpServer::input_tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        assert_eq!(&input, app_tool_names());
        assert!(!input.is_empty());
    }
}
