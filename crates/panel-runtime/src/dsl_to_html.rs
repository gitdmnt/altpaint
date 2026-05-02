//! DSL `PanelTree` → HTML/CSS への純関数翻訳器 (Phase 9E-1)。
//!
//! - 副作用なし。`PanelTree` を入力に `(html, css)` を返す。
//! - 生成 HTML は `data-action="altp:<kind>:<node_id>"` 規約で統一。
//!   - kind = `activate` (Button) / `slider` / `select` / `input` / `color` / `layer-select`
//!   - `data-args` は JSON で補助情報 (Slider の min/max/step、LayerList の index など)。
//! - 9E-2 で `HtmlPanelEngine` 内蔵時に `parse_data_action` 側を `altp:` プレフィックス
//!   対応へ拡張し、PanelEvent (Activate/SetValue/SetText) に解決する。
//! - レイアウト・フォーカス・hover は CSS と Blitz の標準機能で表現する。

use app_core::ColorRgba8;
use panel_api::{PanelNode, PanelTree};

/// 共通 CSS。`HtmlPanelEngine` の user-agent stylesheet として渡される想定。
pub fn default_css() -> &'static str {
    DEFAULT_CSS
}

const DEFAULT_CSS: &str = r#"
/* altpaint DSL panel default stylesheet */
.alt-panel-root { font-family: system-ui, sans-serif; font-size: 12px; color: #ddd; padding: 4px 6px; }
.alt-col { display: flex; flex-direction: column; gap: 4px; }
.alt-row { display: flex; flex-direction: row; gap: 6px; align-items: center; }
.alt-section { margin-top: 4px; padding: 2px 4px; border-top: 1px solid #333; display: flex; flex-direction: column; gap: 4px; }
.alt-section > .alt-section-title { font-weight: bold; padding: 2px 0; }
.alt-text { display: inline-block; }
.alt-text-row { display: flex; gap: 4px; align-items: baseline; }
.alt-text-label { color: #999; }
.alt-btn { background: #333; color: #ddd; border: 1px solid #555; padding: 3px 8px; cursor: pointer; }
.alt-btn:hover { background: #444; }
.alt-btn.alt-active { background: #4a6cff; color: #fff; border-color: #4a6cff; }
.alt-slider { display: flex; flex-direction: column; gap: 2px; }
.alt-slider-row { display: flex; gap: 6px; align-items: center; }
.alt-slider input[type=range] { flex: 1; }
.alt-dropdown { display: flex; gap: 6px; align-items: center; }
.alt-text-input { display: flex; gap: 6px; align-items: center; }
.alt-text-input input { flex: 1; background: #222; color: #ddd; border: 1px solid #555; padding: 2px 4px; }
.alt-color-preview { display: inline-block; width: 16px; height: 16px; border: 1px solid #555; }
.alt-color-wheel { display: flex; gap: 6px; align-items: center; }
.alt-layer-list { list-style: none; padding: 0; margin: 0; }
.alt-layer-item { padding: 2px 4px; cursor: pointer; }
.alt-layer-item:hover { background: #333; }
.alt-layer-item.alt-active { background: #4a6cff; color: #fff; }
.alt-layer-detail { color: #999; font-size: 11px; margin-left: 8px; }
"#;

/// `PanelTree` を `(html, css)` に翻訳する。
///
/// - 返す HTML は完全な `<html><body>` を含み、`HtmlDocument::from_html` 互換。
/// - CSS は `default_css()` と同一 (現状は上書きしない設計)。
pub fn translate_panel_tree(tree: &PanelTree) -> (String, String) {
    let mut body = String::with_capacity(512);
    body.push_str("<div class=\"alt-panel-root\">");
    for node in &tree.children {
        translate_node(node, &mut body);
    }
    body.push_str("</div>");
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body>{}</body></html>",
        escape_html(tree.title),
        body
    );
    (html, default_css().to_string())
}

fn translate_node(node: &PanelNode, out: &mut String) {
    match node {
        PanelNode::Column { id, children } => {
            out.push_str("<div class=\"alt-col\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\">");
            for child in children {
                translate_node(child, out);
            }
            out.push_str("</div>");
        }
        PanelNode::Row { id, children } => {
            out.push_str("<div class=\"alt-row\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\">");
            for child in children {
                translate_node(child, out);
            }
            out.push_str("</div>");
        }
        PanelNode::Section { id, title, children } => {
            // Phase 9G: Blitz/stylo がネストした <details> で primary style 解決に
            // 失敗して panic するため、<div> + 見出しタイトル方式へ切り替えた。
            // open/close UX は当面失われる (post-alpha で再検討)。
            out.push_str("<div class=\"alt-section\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><div class=\"alt-section-title\">");
            out.push_str(&escape_html(title));
            out.push_str("</div>");
            for child in children {
                translate_node(child, out);
            }
            out.push_str("</div>");
        }
        PanelNode::Text { id, text } => {
            out.push_str("<span class=\"alt-text\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\">");
            out.push_str(&escape_html(text));
            out.push_str("</span>");
        }
        PanelNode::Button {
            id,
            label,
            active,
            fill_color,
            ..
        } => {
            let mut class = String::from("alt-btn");
            if *active {
                class.push_str(" alt-active");
            }
            out.push_str("<button id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\" class=\"");
            out.push_str(&class);
            out.push_str("\" data-action=\"altp:activate:");
            out.push_str(&escape_attr(id));
            out.push('"');
            if let Some(color) = fill_color {
                out.push_str(" style=\"background:");
                out.push_str(&color_rgba_css(color));
                out.push('"');
            }
            out.push('>');
            out.push_str(&escape_html(label));
            out.push_str("</button>");
        }
        PanelNode::Slider {
            id,
            label,
            min,
            max,
            value,
            display_value,
            fill_color,
            ..
        } => {
            let display = display_value.unwrap_or(*value);
            let args = format!(r#"{{"min":{min},"max":{max},"step":1}}"#);
            out.push_str("<div class=\"alt-slider\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><div class=\"alt-slider-row\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><span class=\"alt-slider-value\">");
            out.push_str(&display.to_string());
            out.push_str("</span></div><input type=\"range\" id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\" data-action=\"altp:slider:");
            out.push_str(&escape_attr(id));
            out.push_str("\" data-args='");
            out.push_str(&args);
            out.push_str("' min=\"");
            out.push_str(&min.to_string());
            out.push_str("\" max=\"");
            out.push_str(&max.to_string());
            out.push_str("\" value=\"");
            out.push_str(&value.to_string());
            out.push('"');
            if let Some(color) = fill_color {
                out.push_str(" style=\"accent-color:");
                out.push_str(&color_rgba_css(color));
                out.push('"');
            }
            out.push_str("></div>");
        }
        PanelNode::Dropdown {
            id,
            label,
            value,
            options,
            ..
        } => {
            out.push_str("<div class=\"alt-dropdown\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><select id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\" data-action=\"altp:select:");
            out.push_str(&escape_attr(id));
            out.push_str("\">");
            for option in options {
                let selected = if option.value == *value { " selected" } else { "" };
                out.push_str("<option value=\"");
                out.push_str(&escape_attr(&option.value));
                out.push('"');
                out.push_str(selected);
                out.push('>');
                out.push_str(&escape_html(&option.label));
                out.push_str("</option>");
            }
            out.push_str("</select></div>");
        }
        PanelNode::TextInput {
            id,
            label,
            value,
            placeholder,
            input_mode,
            ..
        } => {
            let input_type = match input_mode {
                panel_api::TextInputMode::Numeric => "number",
                panel_api::TextInputMode::Text => "text",
            };
            out.push_str("<div class=\"alt-text-input\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><input type=\"");
            out.push_str(input_type);
            out.push_str("\" id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\" data-action=\"altp:input:");
            out.push_str(&escape_attr(id));
            out.push_str("\" value=\"");
            out.push_str(&escape_attr(value));
            out.push_str("\" placeholder=\"");
            out.push_str(&escape_attr(placeholder));
            out.push_str("\"></div>");
        }
        PanelNode::ColorPreview { id, label, color } => {
            out.push_str("<div class=\"alt-text-row\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><span class=\"alt-color-preview\" style=\"background:");
            out.push_str(&color_rgba_css(color));
            out.push_str("\"></span></div>");
        }
        // TODO(post-alpha): replace with custom color-wheel canvas widget.
        // alpha 期間は HTML 標準 `<input type="color">` で代替し、HSV→#rrggbb 表現に丸める。
        PanelNode::ColorWheel {
            id,
            label,
            hue_degrees,
            saturation,
            value,
            ..
        } => {
            let hex = hsv_to_hex(*hue_degrees, *saturation, *value);
            out.push_str("<div class=\"alt-color-wheel\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><input type=\"color\" id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\" data-action=\"altp:color:");
            out.push_str(&escape_attr(id));
            out.push_str("\" value=\"");
            out.push_str(&hex);
            out.push_str("\"></div>");
        }
        PanelNode::LayerList {
            id,
            label,
            selected_index,
            items,
            ..
        } => {
            out.push_str("<div class=\"alt-layer-list-wrap\" data-altp-id=\"");
            out.push_str(&escape_attr(id));
            out.push_str("\"><span class=\"alt-text-label\">");
            out.push_str(&escape_html(label));
            out.push_str("</span><ul class=\"alt-layer-list\">");
            for (index, item) in items.iter().enumerate() {
                let active = if index == *selected_index { " alt-active" } else { "" };
                out.push_str("<li class=\"alt-layer-item");
                out.push_str(active);
                out.push_str("\" data-index=\"");
                out.push_str(&index.to_string());
                out.push_str("\" data-action=\"altp:layer-select:");
                out.push_str(&escape_attr(id));
                out.push_str("\" data-args='{\"index\":");
                out.push_str(&index.to_string());
                out.push_str("}'>");
                out.push_str(&escape_html(&item.label));
                if !item.detail.is_empty() {
                    out.push_str("<span class=\"alt-layer-detail\">");
                    out.push_str(&escape_html(&item.detail));
                    out.push_str("</span>");
                }
                out.push_str("</li>");
            }
            out.push_str("</ul></div>");
        }
    }
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn color_rgba_css(color: &ColorRgba8) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        color.r,
        color.g,
        color.b,
        f32::from(color.a) / 255.0
    )
}

/// HSV (h: 0-359, s: 0-100, v: 0-100) → `#rrggbb`。
fn hsv_to_hex(h: usize, s: usize, v: usize) -> String {
    let h = (h % 360) as f32;
    let s = (s.min(100) as f32) / 100.0;
    let v = (v.min(100) as f32) / 100.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Command;
    use panel_api::{DropdownOption, HostAction, LayerListItem, PanelNode, PanelTree, TextInputMode};

    fn tree_with(children: Vec<PanelNode>) -> PanelTree {
        PanelTree {
            id: "test.panel",
            title: "Test",
            children,
        }
    }

    #[test]
    fn translate_button_emits_data_action() {
        let tree = tree_with(vec![PanelNode::Button {
            id: "tool.pen".to_string(),
            label: "Pen".to_string(),
            action: HostAction::DispatchCommand(Command::Noop),
            active: false,
            fill_color: None,
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(
            html.contains("data-action=\"altp:activate:tool.pen\""),
            "html missing button data-action: {html}"
        );
        assert!(html.contains(">Pen</button>"));
        assert!(html.contains("class=\"alt-btn\""));
    }

    #[test]
    fn translate_button_active_adds_class() {
        let tree = tree_with(vec![PanelNode::Button {
            id: "b".to_string(),
            label: "On".to_string(),
            action: HostAction::DispatchCommand(Command::Noop),
            active: true,
            fill_color: None,
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("class=\"alt-btn alt-active\""));
    }

    #[test]
    fn translate_section_uses_div_with_title() {
        let tree = tree_with(vec![PanelNode::Section {
            id: "s".to_string(),
            title: "編集".to_string(),
            children: vec![PanelNode::Text {
                id: "t".to_string(),
                text: "hello".to_string(),
            }],
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("<div class=\"alt-section\""));
        assert!(html.contains("<div class=\"alt-section-title\">編集</div>"));
        assert!(html.contains(">hello</span>"));
    }

    #[test]
    fn translate_slider_emits_range_input_with_min_max_step() {
        let tree = tree_with(vec![PanelNode::Slider {
            id: "color.red".to_string(),
            label: "Red".to_string(),
            action: HostAction::DispatchCommand(Command::Noop),
            min: 0,
            max: 255,
            value: 128,
            display_value: None,
            fill_color: None,
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("type=\"range\""));
        assert!(html.contains("data-action=\"altp:slider:color.red\""));
        assert!(html.contains(r#"data-args='{"min":0,"max":255,"step":1}'"#));
        assert!(html.contains("min=\"0\""));
        assert!(html.contains("max=\"255\""));
        assert!(html.contains("value=\"128\""));
    }

    #[test]
    fn translate_text_emits_alt_text_class() {
        let tree = tree_with(vec![PanelNode::Text {
            id: "t".to_string(),
            text: "<>&\"".to_string(),
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("class=\"alt-text\""));
        // HTML エスケープ確認
        assert!(html.contains("&lt;&gt;&amp;\""));
    }

    #[test]
    fn translate_dropdown_marks_selected_option() {
        let tree = tree_with(vec![PanelNode::Dropdown {
            id: "d".to_string(),
            label: "Mode".to_string(),
            value: "b".to_string(),
            action: HostAction::DispatchCommand(Command::Noop),
            options: vec![
                DropdownOption { label: "Alpha".into(), value: "a".into() },
                DropdownOption { label: "Beta".into(), value: "b".into() },
            ],
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("data-action=\"altp:select:d\""));
        assert!(html.contains("<option value=\"a\">Alpha</option>"));
        assert!(html.contains("<option value=\"b\" selected>Beta</option>"));
    }

    #[test]
    fn translate_layer_list_emits_data_index() {
        let tree = tree_with(vec![PanelNode::LayerList {
            id: "layers".to_string(),
            label: "Layers".to_string(),
            selected_index: 1,
            action: HostAction::DispatchCommand(Command::Noop),
            items: vec![
                LayerListItem { label: "L0".into(), detail: "".into() },
                LayerListItem { label: "L1".into(), detail: "100%".into() },
            ],
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("data-action=\"altp:layer-select:layers\""));
        assert!(html.contains("data-index=\"0\""));
        assert!(html.contains("data-index=\"1\""));
        assert!(html.contains(r#"data-args='{"index":0}'"#));
        assert!(html.contains(r#"data-args='{"index":1}'"#));
        // selected_index=1 が active
        assert!(html.contains("class=\"alt-layer-item alt-active\" data-index=\"1\""));
        // detail
        assert!(html.contains("<span class=\"alt-layer-detail\">100%</span>"));
    }

    #[test]
    fn translate_text_input_emits_value_and_placeholder() {
        let tree = tree_with(vec![PanelNode::TextInput {
            id: "name".to_string(),
            label: "Name".to_string(),
            value: "altpaint".to_string(),
            placeholder: "type here".to_string(),
            binding_path: "$.name".to_string(),
            action: None,
            input_mode: TextInputMode::Text,
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("data-action=\"altp:input:name\""));
        assert!(html.contains("type=\"text\""));
        assert!(html.contains("value=\"altpaint\""));
        assert!(html.contains("placeholder=\"type here\""));
    }

    #[test]
    fn translate_text_input_numeric_uses_number_type() {
        let tree = tree_with(vec![PanelNode::TextInput {
            id: "n".to_string(),
            label: "N".to_string(),
            value: "42".to_string(),
            placeholder: "".to_string(),
            binding_path: "$.n".to_string(),
            action: None,
            input_mode: TextInputMode::Numeric,
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("type=\"number\""));
    }

    #[test]
    fn translate_color_wheel_emits_input_color() {
        let tree = tree_with(vec![PanelNode::ColorWheel {
            id: "cw".to_string(),
            label: "Color".to_string(),
            hue_degrees: 0,
            saturation: 100,
            value: 100,
            action: HostAction::DispatchCommand(Command::Noop),
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("type=\"color\""));
        assert!(html.contains("data-action=\"altp:color:cw\""));
        // pure red
        assert!(html.contains("value=\"#ff0000\""));
    }

    #[test]
    fn translate_color_preview_renders_rgba_background() {
        let tree = tree_with(vec![PanelNode::ColorPreview {
            id: "cp".to_string(),
            label: "Preview".to_string(),
            color: ColorRgba8::new(10, 20, 30, 128),
        }]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("class=\"alt-color-preview\""));
        assert!(html.contains("background:rgba(10,20,30,0.502)"));
    }

    #[test]
    fn default_css_contains_alt_panel_root() {
        let css = default_css();
        assert!(css.contains(".alt-panel-root"));
        assert!(css.contains(".alt-btn"));
        assert!(css.contains(".alt-section"));
    }

    #[test]
    fn translate_panel_tree_wraps_in_alt_panel_root() {
        let tree = tree_with(vec![]);
        let (html, _) = translate_panel_tree(&tree);
        assert!(html.contains("<div class=\"alt-panel-root\"></div>"));
        assert!(html.contains("<title>Test</title>"));
    }

    #[test]
    fn translate_complex_tree_does_not_panic() {
        let tree = tree_with(vec![
            PanelNode::Section {
                id: "s".into(),
                title: "Section".into(),
                children: vec![
                    PanelNode::Row {
                        id: "r".into(),
                        children: vec![
                            PanelNode::Text { id: "t1".into(), text: "L".into() },
                            PanelNode::Button {
                                id: "b".into(),
                                label: "Go".into(),
                                action: HostAction::DispatchCommand(Command::Noop),
                                active: false,
                                fill_color: Some(ColorRgba8::new(255, 0, 0, 255)),
                            },
                        ],
                    },
                    PanelNode::Slider {
                        id: "sl".into(),
                        label: "Lvl".into(),
                        action: HostAction::DispatchCommand(Command::Noop),
                        min: 0,
                        max: 10,
                        value: 5,
                        display_value: Some(50),
                        fill_color: None,
                    },
                ],
            },
            PanelNode::Column {
                id: "c".into(),
                children: vec![PanelNode::ColorPreview {
                    id: "cp".into(),
                    label: "Col".into(),
                    color: ColorRgba8::new(0, 0, 0, 255),
                }],
            },
        ]);
        let (html, css) = translate_panel_tree(&tree);
        assert!(!html.is_empty());
        assert!(!css.is_empty());
        // display_value=50 が表示されている
        assert!(html.contains(">50</span>"));
        // fill_color が背景に反映
        assert!(html.contains("style=\"background:rgba(255,0,0,1.000)\""));
    }
}
