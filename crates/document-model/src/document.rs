use serde::{Deserialize, Serialize};

use crate::DocumentCommand;
use editor_state::{EditorSession, SessionCommand, ToolKind};
use geometry::KomaLocalPoint;
use raster::{BlendMode, RgbaBitmap as CanvasBitmap};

mod layer_ops;

use self::layer_ops::ensure_koma_layers;

pub const DEFAULT_PAGE_WIDTH: usize = 2894;
pub const DEFAULT_PAGE_HEIGHT: usize = 4093;

/// ページ 1 辺の最大ピクセル数。
pub const MAX_PAGE_DIMENSION: usize = 8192;
/// ページ全体の最大ピクセル数。
pub const MAX_PAGE_PIXELS: usize = 16_777_216;

/// `"WIDTHxHEIGHT"` 形式の文字列をページ寸法へ解釈する。
///
/// 区切りは `x` / `×` / `,` / `;` / 空白を許容し、上限
/// (`MAX_PAGE_DIMENSION` / `MAX_PAGE_PIXELS`) を超える寸法や 0 は `None`。
pub fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }

    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_PAGE_DIMENSION
        || height > MAX_PAGE_DIMENSION
        || width.saturating_mul(height) > MAX_PAGE_PIXELS
    {
        return None;
    }

    Some((width, height))
}

fn default_active_page_index() -> usize {
    0
}

fn default_active_koma_index() -> usize {
    0
}

fn default_page_width() -> usize {
    DEFAULT_PAGE_WIDTH
}

fn default_page_height() -> usize {
    DEFAULT_PAGE_HEIGHT
}

/// 作品を識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkId(pub u64);

/// ページを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// コマを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KomaId(pub u64);

/// レイヤーノードを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerNodeId(pub u64);

/// アプリケーションの編集状態全体を表すルートドキュメント。
///
/// 単一の `Work` (作品コンテンツ) と、ツール・ペン・色・表示変換などの一過性編集状態
/// (`EditorSession`) を保持する。作品データとセッション状態の責務はここで分離されており、
/// セッション状態の整合・適用ロジックは [`EditorSession`] が担う。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 現在編集中の作品。
    pub work: Work,
    /// 現在アクティブなページ index。
    #[serde(default = "default_active_page_index")]
    pub active_page_index: usize,
    /// 現在アクティブなコマ index。
    #[serde(default = "default_active_koma_index")]
    pub active_koma_index: usize,
    /// エディタの一過性編集状態 (ツール/色/ペン/ビュー)。
    #[serde(default)]
    pub session: EditorSession,
}

/// 漫画作品全体を表す最小単位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work {
    /// 作品ID。
    pub id: WorkId,
    /// 表示用タイトル。
    pub title: String,
    /// ページ列。
    pub pages: Vec<Page>,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            id: WorkId(1),
            title: "Untitled".to_string(),
            pages: vec![Page::default()],
        }
    }
}

/// 作品を構成するページ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// ページID。
    pub id: PageId,
    /// ページの基準幅。
    #[serde(default = "default_page_width")]
    pub width: usize,
    /// ページの基準高さ。
    #[serde(default = "default_page_height")]
    pub height: usize,
    /// ページ内に含まれるコマ列。
    pub komas: Vec<Koma>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            id: PageId(1),
            width: default_page_width(),
            height: default_page_height(),
            komas: vec![Koma::default()],
        }
    }
}

/// ページ内のコマ矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KomaBounds {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl KomaBounds {
    pub fn full_page(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: width.max(1),
            height: height.max(1),
        }
    }

    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width)
            && y < self.y.saturating_add(self.height)
    }

    pub fn contains_canvas_point(self, point: geometry::PagePoint) -> bool {
        self.contains(point.x, point.y)
    }

    pub fn canvas_to_koma_local(
        self,
        point: geometry::PagePoint,
    ) -> Option<geometry::KomaLocalPoint> {
        self.contains_canvas_point(point)
            .then_some(geometry::KomaLocalPoint::new(
                point.x.saturating_sub(self.x),
                point.y.saturating_sub(self.y),
            ))
    }

    pub fn clamp_canvas_point(self, point: geometry::PagePoint) -> Option<geometry::PagePoint> {
        if self.is_empty() {
            return None;
        }

        let max_x = self.x.saturating_add(self.width.saturating_sub(1));
        let max_y = self.y.saturating_add(self.height.saturating_sub(1));
        Some(geometry::PagePoint::new(
            point.x.clamp(self.x, max_x),
            point.y.clamp(self.y, max_y),
        ))
    }

    pub fn koma_local_to_canvas(
        self,
        point: geometry::KomaLocalPoint,
    ) -> Option<geometry::PagePoint> {
        (point.x < self.width && point.y < self.height).then_some(geometry::PagePoint::new(
            self.x.saturating_add(point.x),
            self.y.saturating_add(point.y),
        ))
    }
}

impl Default for KomaBounds {
    fn default() -> Self {
        Self::full_page(DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

/// 漫画のコマを表す最小単位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Koma {
    /// コマID。
    pub id: KomaId,
    /// ページ内でのコマ矩形。
    #[serde(default)]
    pub bounds: KomaBounds,
    /// レイヤー列の合成結果キャッシュ (`layers` から導出される派生データ)。
    pub composite_cache: CanvasBitmap,
    /// コマを構成するラスタレイヤー列。index 0 が最下層。
    #[serde(default)]
    pub layers: Vec<RasterLayer>,
    /// 現在描画対象として選択されているレイヤー index。
    #[serde(default)]
    pub active_layer_index: usize,
    /// これまでに作成されたレイヤー数。
    #[serde(default = "default_created_layer_count")]
    pub created_layer_count: u64,
}

impl Default for Koma {
    fn default() -> Self {
        Self::new_blank(KomaId(1), DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

impl Koma {
    pub fn new_blank(id: KomaId, width: usize, height: usize) -> Self {
        let background = RasterLayer::background(
            LayerNodeId(1),
            "Layer 1".to_string(),
            width.max(1),
            height.max(1),
        );
        Self {
            id,
            bounds: KomaBounds::full_page(width, height),
            composite_cache: background.bitmap.clone(),
            layers: vec![background],
            active_layer_index: 0,
            created_layer_count: 1,
        }
    }
}

const fn default_created_layer_count() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl LayerMask {
    /// 指定座標のマスク alpha を返す。範囲外は 0 (完全マスク) を返す。
    pub fn alpha_at(&self, x: usize, y: usize) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.alpha[y * self.width + x]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterLayer {
    pub id: LayerNodeId,
    pub name: String,
    #[serde(default = "default_layer_visible")]
    pub visible: bool,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub bitmap: CanvasBitmap,
    #[serde(default)]
    pub mask: Option<LayerMask>,
}

fn default_layer_visible() -> bool {
    true
}

impl RasterLayer {
    fn background(id: LayerNodeId, name: String, width: usize, height: usize) -> Self {
        Self {
            id,
            name,
            visible: true,
            blend_mode: BlendMode::Normal,
            bitmap: CanvasBitmap::opaque_white(width, height),
            mask: None,
        }
    }

    fn transparent(id: LayerNodeId, name: String, width: usize, height: usize) -> Self {
        Self {
            id,
            name,
            visible: true,
            blend_mode: BlendMode::Normal,
            bitmap: CanvasBitmap::transparent(width, height),
            mask: None,
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new(DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

impl Document {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);

        Self {
            work: Work {
                pages: vec![Page {
                    width,
                    height,
                    komas: vec![Koma::new_blank(KomaId(1), width, height)],
                    ..Page::default()
                }],
                ..Work::default()
            },
            active_page_index: default_active_page_index(),
            active_koma_index: default_active_koma_index(),
            session: EditorSession::default(),
        }
    }

    pub fn active_page_index(&self) -> usize {
        self.active_page_index
            .min(self.work.pages.len().saturating_sub(1))
    }

    pub fn active_koma_index(&self) -> usize {
        self.active_page()
            .map(|page| {
                self.active_koma_index
                    .min(page.komas.len().saturating_sub(1))
            })
            .unwrap_or(0)
    }

    pub fn active_page(&self) -> Option<&Page> {
        self.work.pages.get(self.active_page_index())
    }

    pub fn active_page_mut(&mut self) -> Option<&mut Page> {
        let index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        self.work.pages.get_mut(index)
    }

    pub fn active_koma(&self) -> Option<&Koma> {
        let koma_index = self.active_koma_index();
        self.active_page()
            .and_then(|page| page.komas.get(koma_index))
    }

    pub fn active_koma_mut(&mut self) -> Option<&mut Koma> {
        let page_index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        let koma_index = self.active_koma_index;
        self.work.pages.get_mut(page_index).and_then(|page| {
            let clamped_index = koma_index.min(page.komas.len().saturating_sub(1));
            page.komas.get_mut(clamped_index)
        })
    }

    pub fn active_bitmap(&self) -> Option<&CanvasBitmap> {
        self.active_koma().map(|koma| &koma.composite_cache)
    }

    pub fn active_layer_bitmap(&self) -> Option<&CanvasBitmap> {
        let koma = self.active_koma()?;
        koma
            .layers
            .get(
                koma
                    .active_layer_index
                    .min(koma.layers.len().saturating_sub(1)),
            )
            .map(|layer| &layer.bitmap)
    }

    pub fn active_layer_is_background(&self) -> Option<bool> {
        let koma = self.active_koma()?;
        Some(koma.active_layer_index == 0)
    }

    pub fn active_koma_contains_canvas_point(&self, point: geometry::PagePoint) -> bool {
        self.active_koma_bounds()
            .is_some_and(|bounds| bounds.contains_canvas_point(point))
    }

    pub fn active_koma_contains_local_point(&self, point: KomaLocalPoint) -> bool {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.koma_local_to_canvas(point))
            .is_some()
    }

    pub fn active_koma_canvas_to_local(
        &self,
        point: geometry::PagePoint,
    ) -> Option<KomaLocalPoint> {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.canvas_to_koma_local(point))
    }

    pub fn active_koma_local_to_canvas(
        &self,
        point: KomaLocalPoint,
    ) -> Option<geometry::PagePoint> {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.koma_local_to_canvas(point))
    }

    pub fn active_koma_bounds(&self) -> Option<KomaBounds> {
        self.active_koma().map(|koma| koma.bounds)
    }

    pub fn active_page_koma_count(&self) -> usize {
        self.active_page()
            .map(|page| page.komas.len())
            .unwrap_or(0)
    }

    pub fn active_page_dimensions(&self) -> (usize, usize) {
        self.active_page()
            .map(|page| (page.width.max(1), page.height.max(1)))
            .unwrap_or((1, 1))
    }

    pub fn select_koma(&mut self, index: usize) {
        let page_index = self.active_page_index();
        if let Some(page) = self.work.pages.get(page_index) {
            self.active_koma_index = index.min(page.komas.len().saturating_sub(1));
        }
    }

    pub fn select_next_koma(&mut self) {
        if let Some(page) = self.active_page() {
            let koma_count = page.komas.len().max(1);
            self.active_koma_index = (self.active_koma_index() + 1) % koma_count;
        }
    }

    pub fn select_previous_koma(&mut self) {
        if let Some(page) = self.active_page() {
            let koma_count = page.komas.len().max(1);
            self.active_koma_index = (self.active_koma_index() + koma_count - 1) % koma_count;
        }
    }

    pub fn add_koma(&mut self) {
        let next_id = next_koma_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };

        let next_count = page.komas.len().saturating_add(1);
        let next_bounds = default_koma_grid_bounds(page.width, page.height, next_count);
        let new_bounds = next_bounds
            .last()
            .copied()
            .unwrap_or_else(|| KomaBounds::full_page(page.width, page.height));
        let mut koma = Koma::new_blank(next_id, new_bounds.width, new_bounds.height);
        koma.bounds = new_bounds;
        page.komas.push(koma);
        relayout_page_komas(page);
        self.active_koma_index = page.komas.len().saturating_sub(1);
        self.focus_active_koma_view();
    }

    pub fn create_koma(&mut self, bounds: KomaBounds) {
        let next_id = next_koma_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        let Some(bounds) = clamp_koma_bounds(bounds, page.width, page.height) else {
            return;
        };

        let mut koma = Koma::new_blank(next_id, bounds.width, bounds.height);
        koma.bounds = bounds;
        page.komas.push(koma);
        self.active_koma_index = page.komas.len().saturating_sub(1);
        self.focus_active_koma_view();
    }

    pub fn remove_active_koma(&mut self) {
        let page_index = self.active_page_index();
        let active_koma_index = self.active_koma_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        if page.komas.len() <= 1 {
            return;
        }
        page.komas.remove(active_koma_index);
        relayout_page_komas(page);
        self.active_koma_index = active_koma_index.min(page.komas.len().saturating_sub(1));
        self.focus_active_koma_view();
    }

    pub fn focus_active_koma_view(&mut self) {
        self.session.reset_view();
    }

    /// ロード後のドキュメント不変条件を修復する。
    ///
    /// ツール状態の整合、空のページ列・コマ列の補完、各 index の clamp、
    /// 空 bounds コマの再レイアウト、レイヤー列の補完を行う。
    pub fn normalize_after_load(&mut self) {
        // セッションのツール整合は `Document` には kind 情報を持たないため、
        // セッションが保持する `active_tool_id` を真実として既定 (Pen) で補修する。
        self.session.ensure_tool_state(ToolKind::default());
        if self.work.pages.is_empty() {
            self.work.pages.push(Page::default());
        }
        self.active_page_index = self.active_page_index();
        for page in &mut self.work.pages {
            page.width = page.width.max(1);
            page.height = page.height.max(1);
            if page.komas.is_empty() {
                page.komas
                    .push(Koma::new_blank(KomaId(1), page.width, page.height));
            }
            let needs_relayout = page.komas.iter().any(|koma| koma.bounds.is_empty());
            for koma in &mut page.komas {
                if koma.bounds.is_empty() {
                    koma.bounds = KomaBounds::full_page(page.width, page.height);
                }
                ensure_koma_layers(koma);
            }
            if needs_relayout {
                relayout_page_komas(page);
            }
        }
        self.active_koma_index = self.active_koma_index();
    }

    /// 純粋なドキュメント変異コマンドをドキュメント状態へ適用する。
    ///
    /// レイヤー・コマ・ドキュメント差し替えなど作品データのみを書き換える。
    /// ツール/色/ペン/ビューの変更は [`Document::apply_session_command`] が担う。
    pub fn apply(&mut self, command: &DocumentCommand) {
        match command {
            DocumentCommand::Noop => {}
            DocumentCommand::CreateKoma {
                x,
                y,
                width,
                height,
            } => {
                self.create_koma(KomaBounds {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                });
            }
            DocumentCommand::AddRasterLayer => {
                self.add_raster_layer();
            }
            DocumentCommand::RemoveActiveLayer => {
                self.remove_active_layer();
            }
            DocumentCommand::SelectLayer { id } => {
                self.select_layer_by_id(*id);
            }
            DocumentCommand::RenameActiveLayer { name } => {
                self.rename_active_layer(name);
            }
            DocumentCommand::MoveLayer { from_id, to_id } => {
                self.move_layer_by_id(*from_id, *to_id);
            }
            DocumentCommand::SelectNextLayer => {
                self.select_next_layer();
            }
            DocumentCommand::CycleActiveLayerBlendMode => {
                self.cycle_active_layer_blend_mode();
            }
            DocumentCommand::SetActiveLayerBlendMode { mode } => {
                self.set_active_layer_blend_mode(mode.clone());
            }
            DocumentCommand::ToggleActiveLayerVisibility => {
                self.toggle_active_layer_visibility();
            }
            DocumentCommand::AddKoma => {
                self.add_koma();
            }
            DocumentCommand::RemoveActiveKoma => {
                self.remove_active_koma();
            }
            DocumentCommand::SelectKoma { index } => {
                self.select_koma(*index);
                self.focus_active_koma_view();
            }
            DocumentCommand::SelectNextKoma => {
                self.select_next_koma();
                self.focus_active_koma_view();
            }
            DocumentCommand::SelectPreviousKoma => {
                self.select_previous_koma();
                self.focus_active_koma_view();
            }
            DocumentCommand::FocusActiveKoma => {
                self.focus_active_koma_view();
            }
            DocumentCommand::NewDocumentSized { width, height } => {
                *self = Document::new(*width, *height);
            }
        }
    }

    /// エディタセッションコマンド (ツール/色/ペン/ビュー) をセッションへ適用する。
    ///
    /// 作品データには触れない。実体は [`EditorSession::apply_session_command`]。
    pub fn apply_session_command(&mut self, command: &SessionCommand) {
        self.session.apply_session_command(command);
    }
}

fn next_koma_id(pages: &[Page]) -> KomaId {
    let next = pages
        .iter()
        .flat_map(|page| page.komas.iter())
        .map(|koma| koma.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    KomaId(next)
}

fn default_koma_grid_bounds(
    page_width: usize,
    page_height: usize,
    koma_count: usize,
) -> Vec<KomaBounds> {
    let koma_count = koma_count.max(1);
    if koma_count == 1 {
        return vec![KomaBounds::full_page(page_width, page_height)];
    }

    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let columns = (koma_count as f32).sqrt().ceil() as usize;
    let rows = koma_count.div_ceil(columns);
    let margin_x = ((page_width as f32 * 0.04).round() as usize).clamp(12, 96);
    let margin_y = ((page_height as f32 * 0.04).round() as usize).clamp(12, 96);
    let gap_x = ((page_width as f32 * 0.015).round() as usize).clamp(8, 48);
    let gap_y = ((page_height as f32 * 0.015).round() as usize).clamp(8, 48);
    let available_width = page_width
        .saturating_sub(margin_x * 2)
        .saturating_sub(gap_x * columns.saturating_sub(1));
    let available_height = page_height
        .saturating_sub(margin_y * 2)
        .saturating_sub(gap_y * rows.saturating_sub(1));
    let cell_width = (available_width / columns.max(1)).max(64);
    let cell_height = (available_height / rows.max(1)).max(64);

    (0..koma_count)
        .map(|index| {
            let row = index / columns.max(1);
            let column = index % columns.max(1);
            KomaBounds {
                x: margin_x + column * (cell_width + gap_x),
                y: margin_y + row * (cell_height + gap_y),
                width: cell_width.min(page_width.max(1)),
                height: cell_height.min(page_height.max(1)),
            }
        })
        .collect()
}

fn relayout_page_komas(page: &mut Page) {
    let bounds = default_koma_grid_bounds(page.width, page.height, page.komas.len());
    for (koma, next_bounds) in page.komas.iter_mut().zip(bounds.into_iter()) {
        resize_koma_to_bounds(koma, next_bounds.width, next_bounds.height);
        koma.bounds = next_bounds;
    }
}

fn clamp_koma_bounds(
    bounds: KomaBounds,
    page_width: usize,
    page_height: usize,
) -> Option<KomaBounds> {
    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let x = bounds.x.min(page_width.saturating_sub(1));
    let y = bounds.y.min(page_height.saturating_sub(1));
    let max_width = page_width.saturating_sub(x);
    let max_height = page_height.saturating_sub(y);
    let width = bounds.width.min(max_width);
    let height = bounds.height.min(max_height);
    (width > 0 && height > 0).then_some(KomaBounds {
        x,
        y,
        width,
        height,
    })
}

fn resize_koma_to_bounds(koma: &mut Koma, width: usize, height: usize) {
    let width = width.max(1);
    let height = height.max(1);
    if koma.composite_cache.width == width && koma.composite_cache.height == height {
        return;
    }

    ensure_koma_layers(koma);
    koma.with_layers_mut(|koma| {
        for layer in &mut koma.layers {
            layer.bitmap = resize_bitmap_nearest(&layer.bitmap, width, height);
            if let Some(mask) = layer.mask.as_mut() {
                *mask = resize_mask_nearest(mask, width, height);
            }
        }
    });
}

fn resize_bitmap_nearest(bitmap: &CanvasBitmap, width: usize, height: usize) -> CanvasBitmap {
    let width = width.max(1);
    let height = height.max(1);
    if bitmap.width == width && bitmap.height == height {
        return bitmap.clone();
    }

    let mut resized = CanvasBitmap::transparent(width, height);
    for y in 0..height {
        let source_y = (((y as f32 / height as f32) * bitmap.height as f32).floor() as usize)
            .min(bitmap.height.saturating_sub(1));
        for x in 0..width {
            let source_x = (((x as f32 / width as f32) * bitmap.width as f32).floor() as usize)
                .min(bitmap.width.saturating_sub(1));
            let source_index = (source_y * bitmap.width + source_x) * 4;
            let target_index = (y * width + x) * 4;
            resized.pixels[target_index..target_index + 4]
                .copy_from_slice(&bitmap.pixels[source_index..source_index + 4]);
        }
    }
    resized
}

fn resize_mask_nearest(mask: &LayerMask, width: usize, height: usize) -> LayerMask {
    let width = width.max(1);
    let height = height.max(1);
    if mask.width == width && mask.height == height {
        return mask.clone();
    }

    let mut alpha = vec![0; width.saturating_mul(height)];
    for y in 0..height {
        let source_y = (((y as f32 / height as f32) * mask.height as f32).floor() as usize)
            .min(mask.height.saturating_sub(1));
        for x in 0..width {
            let source_x = (((x as f32 / width as f32) * mask.width as f32).floor() as usize)
                .min(mask.width.saturating_sub(1));
            alpha[y * width + x] = mask.alpha_at(source_x, source_y);
        }
    }

    LayerMask {
        width,
        height,
        alpha,
    }
}

#[cfg(test)]
mod tests;
