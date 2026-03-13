//! `Document` 内のペン選択状態の整合ロジックをまとめる。
//!
//! プリセット循環・サイズ補正・現在ツールに応じた描画サイズ決定を
//! ドキュメント本体から切り離して保守しやすくする。

use super::{Document, ToolKind, default_pen_presets, default_pen_size};

impl Document {
    /// ペン preset を順送りで切り替える。
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

    /// ペン 状態 が満たされるよう整える。
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

    /// ツール 状態 が満たされるよう整える。
    pub(super) fn ensure_tool_state(&mut self) {
        if self.tool_catalog.is_empty() {
            self.tool_catalog = super::default_tool_catalog();
        }

        if let Some(tool_definition) = self
            .tool_catalog
            .iter()
            .find(|tool| tool.id == self.active_tool_id)
            .cloned()
        {
            self.active_tool = tool_definition.kind;
            self.active_tool_id = tool_definition.id;
            return;
        }

        if let Some(tool_definition) = self
            .tool_catalog
            .iter()
            .find(|tool| tool.kind == self.active_tool)
            .cloned()
            .or_else(|| self.tool_catalog.first().cloned())
        {
            self.active_tool = tool_definition.kind;
            self.active_tool_id = tool_definition.id;
        }
    }

    /// アクティブな 描画 サイズ を返す。
    #[allow(dead_code)]
    pub(super) fn active_draw_size(&self) -> u32 {
        self.active_draw_size_with_pressure(1.0)
    }

    /// アクティブな 描画 サイズ with pressure を返す。
    pub(super) fn active_draw_size_with_pressure(&self, pressure: f32) -> u32 {
        let clamped_pressure = pressure.clamp(0.0, 1.0);
        match self.active_tool {
            ToolKind::Eraser => self.active_pen_size.max(1),
            ToolKind::Pen => {
                let Some(preset) = self.active_pen_preset() else {
                    return self.active_pen_size.max(1);
                };
                let base = self.active_pen_size.max(1);
                if !preset.pressure_enabled {
                    return base;
                }
                let scaled = (base as f32 * (0.2 + clamped_pressure * 0.8)).round() as u32;
                scaled.max(1)
            }
            ToolKind::Bucket | ToolKind::LassoBucket | ToolKind::PanelRect => 1,
        }
    }
}
