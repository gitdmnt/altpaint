//! host state の section registry とキャッシュ枠組み (BL-093)。
//!
//! host state はホスト→パネルへ配る状態 JSON。トップレベルキー
//! (`document` / `tool` / `color` / `view` / `history` / `jobs` / `snapshot`
//! / `workspace`) ごとに [`HostStateSection`] を 1 つ登録し、`PanelRuntime` は
//! 合成とキャッシュ枠組みのみを持つ。
//!
//! キャッシュ無効化は **内容 revision ベース** (BL-093)。各セクションは入力から
//! 安価に `revision` を計算し、revision が変わったセクションのみ `build`
//! (高価な JSON シリアライズを含む) を再実行する。revision が一致すれば前回構築した
//! `Value` を再利用する。これにより `serde_json::to_string` の繰り返しを排除する。
//!
//! パネルは meta.json で購読セクションを宣言し、購読セクションの revision が
//! 変わった時のみ再 render する (`HostStateBuild::changed_sections`)。

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use document_model::Document;
use serde_json::{Value, json};

/// `Document` から導出できない host 側の付随状態 (BL-090)。
///
/// 履歴の undo/redo 可否・実行中ジョブ件数・スナップショット件数を 1 つの DTO に
/// まとめる。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub active_jobs: usize,
    pub snapshot_count: usize,
}

/// `build` 呼出側が事前に組み立てた workspace パネル一覧 JSON のデフォルト。
/// 未設定時は空配列を返す。
pub const EMPTY_WORKSPACE_PANELS_JSON: &str = "[]";

/// セクション構築に必要な入力一式。
pub struct HostStateContext<'a> {
    pub document: &'a Document,
    pub host_state: HostState,
    /// 呼出元が事前に組み立てた `[{"id","title","visible"}, ...]` 形式の JSON。
    /// `workspace` セクションへそのまま格納される。
    pub workspace_panels_json: &'a str,
}

/// host state の 1 トップレベルセクション。
///
/// `revision` は `build` より十分に安価でなければならない (高価な JSON シリアライズは
/// `build` 側のみで行う)。revision が変わらない限り `build` はスキップされる。
trait HostStateSection {
    /// host state JSON のトップレベルキー (例: `"document"`)。
    fn key(&self) -> &'static str;

    /// 入力から導出する内容 revision。値が変われば再 build される。
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64;

    /// セクションの JSON を構築する (revision が変わった時のみ呼ばれる)。
    fn build(&self, ctx: &HostStateContext<'_>) -> Value;
}

/// section registry + per-section キャッシュ。
///
/// 旧 `HostStateCache` + `build_host_state` (230 行) を置換する。
pub struct HostStateRegistry {
    sections: Vec<Box<dyn HostStateSection>>,
    /// key → (前回 revision, 前回構築した Value)。
    cache: BTreeMap<&'static str, (u64, Value)>,
}

/// 1 回の host state 構築結果。
pub struct HostStateBuild {
    /// 全セクションを合成したトップレベル JSON。
    pub value: Value,
    /// 今回 revision が変化した (= 再 build した) セクションキー集合。
    /// パネルの購読判定に使う。
    pub changed_sections: Vec<&'static str>,
}

impl HostStateBuild {
    /// `subscribes` のいずれかが今回変化したか。
    /// `subscribes` が空のパネルは「全セクション購読」とみなし、
    /// 1 つでも変化があれば true を返す。
    pub fn affects(&self, subscribes: &[String]) -> bool {
        if subscribes.is_empty() {
            return !self.changed_sections.is_empty();
        }
        self.changed_sections
            .iter()
            .any(|key| subscribes.iter().any(|s| s == key))
    }
}

impl Default for HostStateRegistry {
    fn default() -> Self {
        Self::with_default_sections()
    }
}

impl HostStateRegistry {
    /// 同梱セクションを登録した registry を構築する。
    ///
    /// 登録は一箇所 (ここ) で行う。feature 分散は B7。
    pub fn with_default_sections() -> Self {
        let mut registry = Self {
            sections: Vec::new(),
            cache: BTreeMap::new(),
        };
        registry.register(Box::new(DocumentSection));
        registry.register(Box::new(ToolSection));
        registry.register(Box::new(ColorSection));
        registry.register(Box::new(ViewSection));
        registry.register(Box::new(HistorySection));
        registry.register(Box::new(JobsSection));
        registry.register(Box::new(SnapshotSection));
        registry.register(Box::new(WorkspaceSection));
        registry
    }

    fn register(&mut self, section: Box<dyn HostStateSection>) {
        self.sections.push(section);
    }

    /// 登録されている全セクションキー (登録順)。
    pub fn section_keys(&self) -> Vec<&'static str> {
        self.sections.iter().map(|s| s.key()).collect()
    }

    /// revision キャッシュを使って host state を構築する。
    ///
    /// revision が変わったセクションのみ再 build し、それ以外は前回 Value を再利用する。
    pub fn build(&mut self, ctx: &HostStateContext<'_>) -> HostStateBuild {
        let mut object = serde_json::Map::with_capacity(self.sections.len());
        let mut changed_sections = Vec::new();
        for section in &self.sections {
            let key = section.key();
            let revision = section.revision(ctx);
            let value = match self.cache.get(key) {
                Some((cached_revision, cached_value)) if *cached_revision == revision => {
                    cached_value.clone()
                }
                _ => {
                    let built = section.build(ctx);
                    self.cache.insert(key, (revision, built.clone()));
                    changed_sections.push(key);
                    built
                }
            };
            object.insert(key.to_string(), value);
        }
        HostStateBuild {
            value: Value::Object(object),
            changed_sections,
        }
    }
}

// ---- revision 計算ヘルパ (安価なフィンガープリント) ----

fn hash_f32<H: Hasher>(hasher: &mut H, value: f32) {
    // 正規化して NaN/-0.0 のビット差を吸収する。
    value.to_bits().hash(hasher);
}

/// `Document` 構造 (作品/コマ/レイヤー) の revision。
fn document_revision(ctx: &HostStateContext<'_>) -> u64 {
    let document = ctx.document;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    document.work.title.hash(&mut hasher);
    document.active_page_index().hash(&mut hasher);
    document.active_koma_index().hash(&mut hasher);
    document.work.pages.len().hash(&mut hasher);
    for page in &document.work.pages {
        page.komas.len().hash(&mut hasher);
        for koma in &page.komas {
            koma.bounds.x.hash(&mut hasher);
            koma.bounds.y.hash(&mut hasher);
            koma.bounds.width.hash(&mut hasher);
            koma.bounds.height.hash(&mut hasher);
        }
    }
    if let Some(koma) = document.active_koma() {
        koma.active_layer_index.hash(&mut hasher);
        koma.layers.len().hash(&mut hasher);
        for layer in &koma.layers {
            layer.name.hash(&mut hasher);
            layer.blend_mode.as_str().hash(&mut hasher);
            layer.visible.hash(&mut hasher);
            layer.mask.is_some().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// ツール/ペン状態の revision。
fn tool_revision(ctx: &HostStateContext<'_>) -> u64 {
    let session = &ctx.document.session;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    session.active_tool_id.hash(&mut hasher);
    session.active_child_tool_id.hash(&mut hasher);
    session.active_pen_size.hash(&mut hasher);
    session.active_pen_index().hash(&mut hasher);
    // ツールカタログ (active ツールが変われば children/settings も再構築される)。
    session.tool_catalog.len().hash(&mut hasher);
    // ペンプリセット内容 (tip ビットマップ含む) を取り込む。
    session.pen_presets.len().hash(&mut hasher);
    for preset in &session.pen_presets {
        preset.id.hash(&mut hasher);
        preset.name.hash(&mut hasher);
        preset.plugin_id.hash(&mut hasher);
        preset.size.hash(&mut hasher);
        preset.pressure_enabled.hash(&mut hasher);
        preset.antialias.hash(&mut hasher);
        preset.stabilization.hash(&mut hasher);
        hash_f32(&mut hasher, preset.spacing_percent);
        hash_f32(&mut hasher, preset.rotation_degrees);
        hash_f32(&mut hasher, preset.opacity);
        hash_f32(&mut hasher, preset.flow);
        match &preset.tip {
            None => 0u8.hash(&mut hasher),
            Some(tip) => {
                1u8.hash(&mut hasher);
                tip.width().hash(&mut hasher);
                tip.height().hash(&mut hasher);
                tip_bytes(tip).hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn tip_bytes(tip: &editor_state::PenTipBitmap) -> &[u8] {
    use editor_state::PenTipBitmap;
    match tip {
        PenTipBitmap::AlphaMask8 { data, .. } | PenTipBitmap::Rgba8 { data, .. } => data,
        PenTipBitmap::PngBlob { png, .. } => png,
    }
}

fn color_revision(ctx: &HostStateContext<'_>) -> u64 {
    let color = ctx.document.session.active_color;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    color.r.hash(&mut hasher);
    color.g.hash(&mut hasher);
    color.b.hash(&mut hasher);
    hasher.finish()
}

fn view_revision(ctx: &HostStateContext<'_>) -> u64 {
    let view = &ctx.document.session.view_transform;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_f32(&mut hasher, view.zoom);
    hash_f32(&mut hasher, view.pan_x);
    hash_f32(&mut hasher, view.pan_y);
    hash_f32(&mut hasher, view.rotation_degrees);
    view.flip_x.hash(&mut hasher);
    view.flip_y.hash(&mut hasher);
    hasher.finish()
}

// ---- セクション実装 ----

struct DocumentSection;
impl HostStateSection for DocumentSection {
    fn key(&self) -> &'static str {
        "document"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        document_revision(ctx)
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        let document = ctx.document;
        let active_page = document.active_page();
        let active_koma = document.active_koma();
        let active_layer = active_koma.and_then(|p| p.layers.get(p.active_layer_index));

        let layer_count = active_koma.map(|p| p.layers.len()).unwrap_or(1);
        let active_layer_index = active_koma.map(|p| p.active_layer_index).unwrap_or(0);

        // index 0 が最下層のため逆順で返す（UI の先頭 = 前面レイヤー）。
        // BL-148: 表示順 index ではなく安定 id (`RasterLayer.id`) を含め、選択/並べ替えの
        // request を id 指定にする。これによりパネル側の二重 index 反転を撤去する。
        let layers = active_koma
            .map(|koma| {
                koma.layers
                    .iter()
                    .rev()
                    .map(|layer| {
                        json!({
                            "id": layer.id.0,
                            "name": layer.name,
                            "blend_mode": layer.blend_mode.as_str(),
                            "visible": layer.visible,
                            "masked": layer.mask.is_some(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![json!({ "name": "Layer 1", "blend_mode": "normal", "visible": true, "masked": false })]
            });
        let layers_json = serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());

        // コマ一覧は bounds の生データのみ (ラベル整形はパネル側 = BL-094)。
        let komas = active_page
            .map(|page| {
                page.komas
                    .iter()
                    .map(|koma| {
                        json!({
                            "x": koma.bounds.x,
                            "y": koma.bounds.y,
                            "width": koma.bounds.width,
                            "height": koma.bounds.height,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![json!({ "x": 0, "y": 0, "width": 0, "height": 0 })]);
        let komas_json = serde_json::to_string(&komas).unwrap_or_else(|_| "[]".to_string());

        let page_count = document.work.pages.len();
        let active_page_number = document.active_page_index() + 1;
        let active_koma_number = document.active_koma_index() + 1;
        let active_page_koma_count = document.active_page_koma_count();
        let koma_count = document
            .work
            .pages
            .iter()
            .map(|page| page.komas.len())
            .sum::<usize>();
        let active_layer_name = active_layer
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "<no layer>".to_string());
        // BL-094: bounds は生データで配り、ラベル整形はパネル側 (Wasm) で行う。
        let active_koma_bounds = active_koma.map(|koma| koma.bounds);

        // UI インデックス: UI の先頭が前面なので実モデル index を逆変換する。
        let active_layer_ui_index = if layer_count > 0 {
            layer_count.saturating_sub(1).saturating_sub(active_layer_index)
        } else {
            0
        };

        json!({
            "title": document.work.title,
            "page_count": page_count,
            "koma_count": koma_count,
            "active_page_number": active_page_number,
            "active_page_koma_count": active_page_koma_count,
            "active_koma_index": document.active_koma_index(),
            "active_koma_number": active_koma_number,
            "active_koma_x": active_koma_bounds.map(|b| b.x).unwrap_or(0),
            "active_koma_y": active_koma_bounds.map(|b| b.y).unwrap_or(0),
            "active_koma_width": active_koma_bounds.map(|b| b.width).unwrap_or(0),
            "active_koma_height": active_koma_bounds.map(|b| b.height).unwrap_or(0),
            "active_layer_name": active_layer_name,
            "layer_count": layer_count,
            "active_layer_index": active_layer_ui_index,
            "active_layer_blend_mode": active_layer.map(|layer| layer.blend_mode.as_str()).unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "komas_json": komas_json,
            "layers_json": layers_json,
        })
    }
}

struct ToolSection;
impl HostStateSection for ToolSection {
    fn key(&self) -> &'static str {
        "tool"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        tool_revision(ctx)
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        let session = &ctx.document.session;
        let active_tool_definition = session.active_tool_definition().cloned();
        let active_pen_index = session.active_pen_index();
        let pen_count = session.pen_presets.len();
        let active_pen = session.active_pen_preset().cloned().unwrap_or_default();
        let active_child_tool_id = &session.active_child_tool_id;
        let active_child_tool_label = session
            .active_child_tool_definition()
            .map(|c| c.name.clone())
            .unwrap_or_default();

        let tool_catalog_json =
            serde_json::to_string(&session.tool_catalog).unwrap_or_else(|_| "[]".to_string());
        let active_tool_settings_json =
            serde_json::to_string(session.active_tool_settings()).unwrap_or_else(|_| "[]".to_string());
        let child_tools_json = active_tool_definition
            .as_ref()
            .map(|t| serde_json::to_string(&t.children).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        let pen_presets_json =
            serde_json::to_string(&session.pen_presets).unwrap_or_else(|_| "[]".to_string());

        json!({
            "active": session.active_tool().as_str(),
            "active_id": &session.active_tool_id,
            "active_label": active_tool_definition
                .as_ref()
                .map(|tool| tool.name.clone())
                .unwrap_or_else(|| session.active_tool().as_str().to_string()),
            "catalog_json": tool_catalog_json,
            "active_settings_json": active_tool_settings_json,
            "active_child_tool_id": active_child_tool_id,
            "active_child_tool_label": active_child_tool_label,
            "child_tools_json": child_tools_json,
            "active_provider_plugin_id": session.active_tool_provider_plugin_id().unwrap_or_default(),
            "active_drawing_plugin_id": session.active_tool_drawing_plugin_id().unwrap_or_default(),
            "supports_size": session.active_tool_settings().iter().any(|setting| setting.key == "size"),
            "supports_pressure_enabled": session.active_tool_settings().iter().any(|setting| setting.key == "pressure_enabled"),
            "supports_antialias": session.active_tool_settings().iter().any(|setting| setting.key == "antialias"),
            "supports_stabilization": session.active_tool_settings().iter().any(|setting| setting.key == "stabilization"),
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_presets_json": pen_presets_json,
            "pen_index": active_pen_index,
            "pen_count": pen_count,
            "pen_size": session.active_pen_size,
            "pen_pressure_enabled": active_pen.pressure_enabled,
            "pen_antialias": active_pen.antialias,
            "pen_stabilization": active_pen.stabilization,
        })
    }
}

struct ColorSection;
impl HostStateSection for ColorSection {
    fn key(&self) -> &'static str {
        "color"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        color_revision(ctx)
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        let color = ctx.document.session.active_color;
        json!({
            "active": color.hex_rgb(),
            "red": color.r,
            "green": color.g,
            "blue": color.b,
        })
    }
}

struct ViewSection;
impl HostStateSection for ViewSection {
    fn key(&self) -> &'static str {
        "view"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        view_revision(ctx)
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        let view = &ctx.document.session.view_transform;
        json!({
            "zoom": view.zoom,
            "zoom_milli": (view.zoom * 1000.0).round() as i32,
            "pan_x": view.pan_x.round() as i32,
            "pan_y": view.pan_y.round() as i32,
            "rotation_degrees": view.rotation_degrees.round() as i32,
            "quarter_turns": ((view.rotation_degrees / 90.0).round() as i32).rem_euclid(4),
            "flip_x": view.flip_x,
            "flip_y": view.flip_y,
        })
    }
}

struct HistorySection;
impl HostStateSection for HistorySection {
    fn key(&self) -> &'static str {
        "history"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ctx.host_state.can_undo.hash(&mut hasher);
        ctx.host_state.can_redo.hash(&mut hasher);
        hasher.finish()
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        json!({
            "can_undo": ctx.host_state.can_undo,
            "can_redo": ctx.host_state.can_redo,
        })
    }
}

struct JobsSection;
impl HostStateSection for JobsSection {
    fn key(&self) -> &'static str {
        "jobs"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ctx.host_state.active_jobs.hash(&mut hasher);
        hasher.finish()
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        // 生データのみ (status 文字列整形はパネル側 = BL-094)。
        json!({
            "active": ctx.host_state.active_jobs,
            "queued": 0,
        })
    }
}

struct SnapshotSection;
impl HostStateSection for SnapshotSection {
    fn key(&self) -> &'static str {
        "snapshot"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ctx.host_state.snapshot_count.hash(&mut hasher);
        hasher.finish()
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        let count = ctx.host_state.snapshot_count;
        json!({
            "count": count,
            "storage_status": if count == 0 { "empty" } else { "ok" },
        })
    }
}

struct WorkspaceSection;
impl HostStateSection for WorkspaceSection {
    fn key(&self) -> &'static str {
        "workspace"
    }
    fn revision(&self, ctx: &HostStateContext<'_>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ctx.workspace_panels_json.hash(&mut hasher);
        hasher.finish()
    }
    fn build(&self, ctx: &HostStateContext<'_>) -> Value {
        // 呼出元が組み立て済みの JSON 文字列をそのまま埋め込む。
        json!({
            "panels_json": ctx.workspace_panels_json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_model::{Document, KomaBounds};
    use raster::BlendMode;

    fn ctx<'a>(document: &'a Document, workspace_panels_json: &'a str) -> HostStateContext<'a> {
        HostStateContext {
            document,
            host_state: HostState::default(),
            workspace_panels_json,
        }
    }

    fn build(registry: &mut HostStateRegistry, document: &Document) -> Value {
        registry
            .build(&ctx(document, EMPTY_WORKSPACE_PANELS_JSON))
            .value
    }

    /// BL-093: レイヤー名変更が次回 build の layers_json に反映される (revision キャッシュ)。
    #[test]
    fn layer_rename_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = build(&mut registry, &document);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .name = "Renamed Layer".to_string();

        let second = build(&mut registry, &document);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains("Renamed Layer"),
            "layers_json must reflect the renamed layer: {layers_json}"
        );
    }

    /// BL-093: レイヤー visible 変更が次回 build の layers_json に反映される。
    #[test]
    fn layer_visibility_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = build(&mut registry, &document);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .visible = false;

        let second = build(&mut registry, &document);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains(r#""visible":false"#),
            "layers_json must reflect visibility change: {layers_json}"
        );
    }

    /// BL-093: レイヤー blend_mode 変更が次回 build の layers_json に反映される。
    #[test]
    fn layer_blend_mode_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = build(&mut registry, &document);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .blend_mode = BlendMode::Multiply;

        let second = build(&mut registry, &document);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains("multiply"),
            "layers_json must reflect blend mode change: {layers_json}"
        );
    }

    /// BL-093: コマ bounds 変更が次回 build の komas_json に反映される。
    #[test]
    fn koma_bounds_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = build(&mut registry, &document);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .bounds = KomaBounds {
            x: 7,
            y: 9,
            width: 123,
            height: 45,
        };

        let second = build(&mut registry, &document);
        let komas_json = second["document"]["komas_json"]
            .as_str()
            .expect("komas_json is string");
        // BL-094: 生データのみ (ラベル整形はパネル側)。
        assert!(
            komas_json.contains(r#""x":7"#)
                && komas_json.contains(r#""y":9"#)
                && komas_json.contains(r#""width":123"#)
                && komas_json.contains(r#""height":45"#),
            "komas_json must reflect bounds change as raw data: {komas_json}"
        );
    }

    /// BL-093: ペンプリセットの内容編集 (件数・active index 不変) が次回 build の
    /// pen_presets_json に反映される (revision が tip/内容を取り込む)。
    #[test]
    fn pen_preset_content_edit_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = build(&mut registry, &document);

        document.session.pen_presets[0].name = "Edited Pen".to_string();

        let second = build(&mut registry, &document);
        let pen_presets_json = second["tool"]["pen_presets_json"]
            .as_str()
            .expect("pen_presets_json is string");
        assert!(
            pen_presets_json.contains("Edited Pen"),
            "pen_presets_json must reflect preset content edit: {pen_presets_json}"
        );
    }

    /// BL-093: 変化が無ければ changed_sections は空 (2 回目)。
    /// view のみ変えれば view セクションだけが changed になる。
    #[test]
    fn unchanged_build_reports_no_changed_sections() {
        let document = Document::default();
        let mut registry = HostStateRegistry::default();
        let first = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));
        // 初回は全セクションが changed。
        assert_eq!(
            first.changed_sections.len(),
            registry.section_keys().len(),
            "first build rebuilds all sections"
        );

        let second = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));
        assert!(
            second.changed_sections.is_empty(),
            "unchanged build must report no changed sections, got {:?}",
            second.changed_sections
        );
    }

    /// BL-093: view のみ変えると view セクションだけが changed になる (購読粒度の根拠)。
    #[test]
    fn view_change_only_marks_view_section_changed() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));

        document.session.view_transform.zoom *= 2.0;

        let second = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));
        assert_eq!(
            second.changed_sections,
            vec!["view"],
            "only the view section should change"
        );
    }

    /// BL-093: 購読セクションの変化のみが `affects` で true になる。
    #[test]
    fn affects_respects_subscriptions() {
        let mut document = Document::default();
        let mut registry = HostStateRegistry::default();
        let _ = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));

        document.session.active_color.r = document.session.active_color.r.wrapping_add(1);
        let build = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));

        assert!(build.affects(&["color".to_string()]));
        assert!(!build.affects(&["view".to_string(), "tool".to_string()]));
        // 空購読は「全セクション購読」: 変化が 1 つでもあれば true。
        assert!(build.affects(&[]));
    }

    /// build が `workspace.panels_json` を渡された文字列のまま出力する。
    #[test]
    fn host_state_emits_workspace_panels_json() {
        let document = Document::default();
        let mut registry = HostStateRegistry::default();
        let workspace_panels_json = r#"[{"id":"builtin.foo","title":"Foo","visible":true}]"#;

        let build = registry.build(&HostStateContext {
            document: &document,
            host_state: HostState::default(),
            workspace_panels_json,
        });

        let emitted = build.value["workspace"]["panels_json"]
            .as_str()
            .expect("workspace.panels_json must be present");
        assert_eq!(emitted, workspace_panels_json);
    }

    /// workspace_panels_json が空 (デフォルト) の場合は空配列文字列がそのまま出る。
    #[test]
    fn host_state_emits_empty_workspace_panels_json_when_absent() {
        let document = Document::default();
        let mut registry = HostStateRegistry::default();

        let build = registry.build(&ctx(&document, EMPTY_WORKSPACE_PANELS_JSON));

        let emitted = build.value["workspace"]["panels_json"]
            .as_str()
            .expect("workspace.panels_json must be present");
        assert_eq!(emitted, "[]");
    }

    /// BL-094: プレゼンテーション文字列が host state に含まれず、生データのみが出る。
    #[test]
    fn host_state_has_no_presentation_strings() {
        let document = Document::default();
        let mut registry = HostStateRegistry::default();
        let value = build(&mut registry, &document);

        assert!(
            value["document"].get("active_koma_label").is_none(),
            "active_koma_label (presentation string) must be removed"
        );
        assert!(
            value["document"].get("active_koma_bounds").is_none(),
            "active_koma_bounds (presentation string) must be removed"
        );
        assert!(
            value["jobs"].get("status").is_none(),
            "jobs.status (presentation string) must be removed"
        );

        // 生データは存在する。
        assert!(value["document"]["active_koma_width"].is_number());
        assert!(value["document"]["active_koma_x"].is_number());
        assert!(value["jobs"]["active"].is_number());

        // komas_json も生データ (name/detail の整形文字列を含まない)。
        let komas_json = value["document"]["komas_json"]
            .as_str()
            .expect("komas_json is string");
        assert!(
            !komas_json.contains("コマ ") && !komas_json.contains('×'),
            "komas_json must not contain formatted labels: {komas_json}"
        );
    }
}
