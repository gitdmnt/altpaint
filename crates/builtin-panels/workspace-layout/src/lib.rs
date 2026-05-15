//! `builtin.workspace-layout` パネル (Phase 12 / ADR 014)。
//!
//! ホスト snapshot の `workspace.panels_json` から登録パネル一覧を受け取り、
//! チェックボックス UI を生成する。チェック切替で
//! `workspace_layout.set_panel_visibility` を emit してホスト側で可視性を反映する。

use plugin_sdk::{
    dom::{html_escape, query_selector, set_inner_html},
    host,
    runtime::{emit_service, event_string},
    services,
};

/// `workspace.panels_json` 内 1 エントリのパネル。
#[derive(Default, serde::Deserialize)]
struct PanelEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    visible: bool,
}

const SELF_PANEL_ID: &str = "builtin.workspace-layout";

/// `workspace.panels_json` を `<li>` 列に変換する。
/// 自身 (builtin.workspace-layout) は出力しない。
fn render_panel_list(workspace_panels_json: &str) -> String {
    let panels: Vec<PanelEntry> = serde_json::from_str(workspace_panels_json).unwrap_or_default();
    let mut out = String::new();
    for panel in panels.iter().filter(|panel| panel.id != SELF_PANEL_ID) {
        let id_attr = html_escape(&panel.id);
        let title = html_escape(&panel.title);
        let next_state: i32 = if panel.visible { 0 } else { 1 };
        let checked = if panel.visible { " checked=\"checked\"" } else { "" };
        let data_args = format!(r#"{{"value":{},"panel_id":"{}"}}"#, next_state, id_attr);
        out.push_str(&format!(
            r#"<li><label><input type="checkbox" id="workspace.toggle.{id}" data-action="altp:activate:set_visibility" data-args='{args}'{checked}/><span>{title}</span></label></li>"#,
            id = id_attr,
            args = data_args,
            checked = checked,
            title = title,
        ));
    }
    out
}

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    if let Some(list) = query_selector("#workspace-panel-list") {
        let json = host::workspace::panels_json();
        set_inner_html(list, &render_panel_list(&json));
    }
}

#[plugin_sdk::panel_handler]
fn set_visibility(value: i32) {
    let panel_id = event_string("panel_id");
    if panel_id.is_empty() {
        return;
    }
    emit_service(&services::workspace_layout::set_panel_visibility(
        panel_id,
        value != 0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        set_visibility(0);
        set_visibility(1);
    }

    #[test]
    fn render_panel_list_emits_data_args_with_panel_id() {
        let json = r#"[{"id":"builtin.tool-palette","title":"ツール","visible":true}]"#;
        let out = render_panel_list(json);
        assert!(
            out.contains(r#"data-action="altp:activate:set_visibility""#),
            "should set the handler name in data-action"
        );
        assert!(
            out.contains(r#"data-args='{"value":0,"panel_id":"builtin.tool-palette"}'"#),
            "should embed both value and panel_id in data-args (out={out})"
        );
        assert!(out.contains("checked=\"checked\""), "visible panel marked as checked");
    }

    #[test]
    fn render_panel_list_emits_value_one_for_hidden_panel() {
        let json = r#"[{"id":"builtin.panel-list","title":"ページ","visible":false}]"#;
        let out = render_panel_list(json);
        assert!(
            out.contains(r#""value":1"#),
            "hidden panel: clicking toggles to visible (value=1)"
        );
        assert!(!out.contains("checked=\"checked\""));
    }

    #[test]
    fn render_panel_list_excludes_self() {
        let json = r#"[{"id":"builtin.workspace-layout","title":"パネル管理","visible":true},{"id":"builtin.tool-palette","title":"ツール","visible":true}]"#;
        let out = render_panel_list(json);
        assert!(
            !out.contains("workspace-layout"),
            "self entry must be excluded from the list (out={out})"
        );
        assert!(out.contains("builtin.tool-palette"));
    }

    #[test]
    fn render_panel_list_escapes_xss_in_title() {
        let json = r#"[{"id":"builtin.foo","title":"<script>alert(1)</script>","visible":true}]"#;
        let out = render_panel_list(json);
        assert!(!out.contains("<script>"), "no raw script in output: {out}");
        assert!(out.contains("&lt;script&gt;"));
    }
}
