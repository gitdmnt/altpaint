//! `HtmlPanelView` の GPU 提示 (BL-100)。
//!
//! 描画先テクスチャ ([`crate::gpu::PanelGpuTarget`]) の所有と再生成、vello scene の
//! 構築、chrome (タイトルバー) の重ね描画、`render_to_texture` の呼出。
//! `vello::Renderer` / `wgpu::Device` / `wgpu::Queue` は保持せず `on_render` 引数で借りる。

use super::{ChromeStyle, HtmlPanelView, LocalRenderSize, RenderOutcome};
use anyrender_vello::VelloScenePainter;
use blitz_paint::paint_scene;

impl HtmlPanelView {
    /// 現在の GPU target への参照（render 後に外部が view を作るため）。
    pub fn gpu_target(&self) -> Option<&crate::gpu::PanelGpuTarget> {
        self.gpu_target.as_ref()
    }

    /// パネルを GPU テクスチャに描画する（責務集約）。
    ///
    /// 動作 (Phase 11):
    /// 1. viewport (画面側) で **描画用ローカル size** を算出: `min(measured_w, viewport_w)` 等。
    ///    `panel_size` 自体は変更しない (ウィンドウ縮小→復元時の往復不変)。
    /// 2. layout_dirty なら local size で `resolve_layout` を走らせる (content size の自動再測定はしない)。
    /// 3. render_dirty なら scene 構築 + chrome 描画 + render_to_texture。
    ///
    /// `chrome` が `Some` なら上端にその高さ・色で chrome 矩形を重ねる。色は呼出側
    /// (テーマ) が決める (BL-099: panel-html はテーマ色をハードコードしない)。
    ///
    /// 戻り値: `RenderOutcome::Rendered(target)` か `Skipped(target)`。
    /// `target` は `gpu_target()` でも取得可能。
    #[allow(clippy::too_many_arguments)]
    pub fn on_render<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        scene_buf: &mut vello::Scene,
        viewport: (u32, u32),
        scale: f32,
        chrome: Option<ChromeStyle>,
    ) -> RenderOutcome<'a> {
        let chrome_height = chrome.map(|c| c.height).unwrap_or(0);
        // viewport クランプ: 描画用 local 変数のみで行い panel_size は変更しない。
        let LocalRenderSize {
            width: local_w,
            height: local_h,
            body_height: body_h,
        } = self.local_render_size(viewport, chrome_height);

        // layout_dirty なら resolve のみ実行 (content size の再測定 + panel_size 更新は廃止)
        if self.layout_dirty {
            self.resolve_layout(local_w, body_h, scale);
            self.layout_dirty = false;
        }

        // GPU target サイズを local size に合わせる
        let target_size_changed = self
            .gpu_target
            .as_ref()
            .map(|t| t.width != local_w || t.height != local_h)
            .unwrap_or(true);
        if target_size_changed {
            self.gpu_target = Some(crate::gpu::PanelGpuTarget::create(device, local_w, local_h));
            self.render_dirty = true;
        }

        if !self.render_dirty {
            return RenderOutcome::Skipped(self.gpu_target.as_ref().expect("target ensured"));
        }

        // scene 構築 + chrome 描画 (色は呼出側注入の chrome.fill_rgba)
        scene_buf.reset();
        self.build_scene_with_offset(scene_buf, local_w, body_h, scale, 0, chrome_height);
        if let Some(chrome) = chrome.filter(|c| c.height > 0) {
            paint_chrome_rect(scene_buf, local_w, chrome.height, chrome.fill_rgba);
        }

        let target = self.gpu_target.as_ref().expect("target ensured");
        let view = target.create_render_view();
        renderer
            .render_to_texture(
                device,
                queue,
                scene_buf,
                &view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: local_w,
                    height: local_h,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .expect("vello render_to_texture failed");

        self.render_dirty = false;
        RenderOutcome::Rendered(self.gpu_target.as_ref().expect("target ensured"))
    }

    /// blitz-paint で `vello::Scene` を埋める（実描画は `on_render`）。
    /// HTML 本体を `(x_offset, y_offset)` ピクセル分ずらして描画する。
    /// ホスト描画タイトルバーを上に重ねるためのオフセット指定に使う。
    pub fn build_scene_with_offset(
        &mut self,
        scene: &mut vello::Scene,
        width: u32,
        height: u32,
        scale: f32,
        x_offset: u32,
        y_offset: u32,
    ) {
        self.resolve_layout(width, height, scale);
        let mut painter = VelloScenePainter::new(scene);
        paint_scene(
            &mut painter,
            &self.document,
            scale as f64,
            width,
            height,
            x_offset,
            y_offset,
        );
    }
}

/// HTML パネル上端のタイトルバー (chrome) を vello シーンに矩形で描画する。
/// 塗り色 `fill_rgba` は呼出側 (テーマ) が決める — panel-html はテーマ色を
/// 知らない (BL-099)。テキスト描画は将来追加。
fn paint_chrome_rect(scene: &mut vello::Scene, width: u32, chrome_height: u32, fill_rgba: [u8; 4]) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::{Color, Fill};
    let rect = Rect::new(0.0, 0.0, width as f64, chrome_height as f64);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(fill_rgba[0], fill_rgba[1], fill_rgba[2], fill_rgba[3]),
        None,
        &rect,
    );
}
