//! `Document` 内のペン選択状態の整合ロジックをまとめる。
//!
//! プリセット循環・サイズ補正・現在ツールに応じた描画サイズ決定を
//! ドキュメント本体から切り離して保守しやすくする。

use super::{default_pen_presets, default_pen_size, Document, ToolKind};

impl Document {
    /// 指定方向へペンプリセットを循環する。
    pub(super) fn cycle_pen_preset(&mut self, delta: isize) {
        self.ensure_pen_state();
        if self.pen_presets.is_empty() {
            return;
        }

        let len = self.pen_presets.len() as isize;
        let next_index = (self.active_pen_index() as isize + delta).rem_euclid(len) as usize;
        let preset = &self.pen_presets[next_index];
        self.active_pen_preset_id = preset.id.clone();
        self.active_pen_size = preset.clamp_size(preset.size);
    }

    /// ペンプリセット列と選択状態の不整合を補正する。
    pub(super) fn ensure_pen_state(&mut self) {
        if self.pen_presets.is_empty() {
            self.pen_presets = default_pen_presets();
        }

        if self
            .pen_presets
            .iter()
            .all(|preset| preset.id != self.active_pen_preset_id)
            && let Some(preset) = self.pen_presets.first()
        {
            self.active_pen_preset_id = preset.id.clone();
        }

        if let Some(preset) = self.active_pen_preset() {
            self.active_pen_size = preset.clamp_size(self.active_pen_size);
        } else {
            self.active_pen_size = default_pen_size();
        }
    }

    /// 現在ツールに応じた実効描画サイズを返す。
    pub(super) fn active_draw_size(&self) -> u32 {
        match self.active_tool {
            ToolKind::Brush | ToolKind::Eraser => 1,
            ToolKind::Pen => self.active_pen_size.max(1),
        }
    }
}
