use crate::config::{collect_persistent_panel_configs, restore_persistent_panel_configs};
use crate::dsl_loader::collect_panel_files_recursive;
use crate::dsl_panel::DslPanelPlugin;
use app_core::Document;
use panel_api::{HostAction, PanelEvent, PanelPlugin, PanelTree, PanelView};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimeDispatchResult {
    pub actions: Vec<HostAction>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimeKeyboardResult {
    pub handled: bool,
    pub actions: Vec<HostAction>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

/// DSL/Wasm panel runtime と registry を保持する。
pub struct PanelRuntime {
    panels: Vec<Box<dyn PanelPlugin>>,
    loaded_panel_ids: Vec<String>,
    panel_tree_cache: BTreeMap<String, PanelTree>,
    persistent_panel_configs: BTreeMap<String, Value>,
}

impl Default for PanelRuntime {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new()
    }
}

impl PanelRuntime {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            loaded_panel_ids: Vec::new(),
            panel_tree_cache: BTreeMap::new(),
            persistent_panel_configs: BTreeMap::new(),
        }
    }

    /// 現在の値を パネル へ変換する。
    pub fn register_panel(&mut self, mut panel: Box<dyn PanelPlugin>) {
        if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
        self.panels
            .retain(|registered| registered.id() != panel.id());
        self.panel_tree_cache.remove(panel.id());
        self.panels.push(panel);
        if let Some(panel) = self.panels.last() {
            self.panel_tree_cache
                .insert(panel.id().to_string(), panel.panel_tree());
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub fn load_panel_directory(&mut self, directory: impl AsRef<Path>) -> Vec<String> {
        let directory = directory.as_ref();
        self.remove_loaded_panels();

        let mut panel_files = Vec::new();
        if collect_panel_files_recursive(directory, &mut panel_files).is_err() {
            return Vec::new();
        }
        panel_files.sort();

        let mut diagnostics = Vec::new();
        for path in panel_files {
            match panel_dsl::load_panel_file(&path) {
                Ok(definition) => {
                    let panel_id = definition.manifest.id.clone();
                    match DslPanelPlugin::from_definition(definition) {
                        Ok(panel) => {
                            self.loaded_panel_ids.push(panel_id);
                            self.register_panel(Box::new(panel));
                        }
                        Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
                    }
                }
                Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
            }
        }

        diagnostics
    }

    /// ドキュメント を現在の状態へ同期する。
    pub fn sync_document(&mut self, document: &Document) -> BTreeSet<String> {
        self.sync_document_subset(document, None)
    }

    /// ドキュメント panels を現在の状態へ同期する。
    pub fn sync_document_panels(
        &mut self,
        document: &Document,
        panel_ids: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        if panel_ids.is_empty() {
            return self.sync_document(document);
        }
        self.sync_document_subset(document, Some(panel_ids))
    }

    /// 現在の パネル 件数 を返す。
    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// 既存データを走査して パネル debug summaries を組み立てる。
    pub fn panel_debug_summaries(&self) -> Vec<(&'static str, &'static str, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id(), panel.title(), panel.debug_summary()))
            .collect()
    }

    /// 既存データを走査して パネル views を組み立てる。
    pub fn panel_views(&self) -> Vec<PanelView> {
        self.panels.iter().map(|panel| panel.view()).collect()
    }

    /// 既存データを走査して パネル trees を組み立てる。
    pub fn panel_trees(&self) -> Vec<PanelTree> {
        self.panels
            .iter()
            .map(|panel| {
                self.panel_tree_cache
                    .get(panel.id())
                    .cloned()
                    .unwrap_or_else(|| panel.panel_tree())
            })
            .collect()
    }

    /// persistent パネル configs を計算して返す。
    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        collect_persistent_panel_configs(&self.panels)
    }

    /// Persistent パネル configs を置き換える。
    pub fn replace_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        restore_persistent_panel_configs(&mut self.panels, &self.persistent_panel_configs);
        self.rebuild_tree_cache();
    }

    /// 現在の値を イベント へ変換する。
    pub fn dispatch_event(&mut self, event: &PanelEvent) -> RuntimeDispatchResult {
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == event_panel_id(event))
        else {
            return RuntimeDispatchResult::default();
        };

        let previous_tree = self
            .panel_tree_cache
            .get(panel.id())
            .cloned()
            .unwrap_or_else(|| panel.panel_tree());
        let actions = panel.handle_event(event);
        let next_tree = panel.panel_tree();
        self.panel_tree_cache
            .insert(panel.id().to_string(), next_tree.clone());
        let mut changed_panel_ids = BTreeSet::new();
        if next_tree != previous_tree {
            changed_panel_ids.insert(panel.id().to_string());
        }
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
        RuntimeDispatchResult {
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// 現在の値を キーボード へ変換する。
    pub fn dispatch_keyboard(
        &mut self,
        shortcut: &str,
        key: &str,
        repeat: bool,
    ) -> RuntimeKeyboardResult {
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let mut handled = false;
        let mut actions = Vec::new();
        let mut changed_panel_ids = BTreeSet::new();
        for panel in &mut self.panels {
            if !panel.handles_keyboard_event() {
                continue;
            }
            let previous_tree = self
                .panel_tree_cache
                .get(panel.id())
                .cloned()
                .unwrap_or_else(|| panel.panel_tree());
            let previous_config = panel.persistent_config();
            let panel_actions = panel.handle_event(&PanelEvent::Keyboard {
                panel_id: panel.id().to_string(),
                shortcut: shortcut.to_string(),
                key: key.to_string(),
                repeat,
            });
            let next_tree = panel.panel_tree();
            self.panel_tree_cache
                .insert(panel.id().to_string(), next_tree.clone());
            let keyboard_handled = !panel_actions.is_empty()
                || next_tree != previous_tree
                || panel.persistent_config() != previous_config;
            if next_tree != previous_tree {
                changed_panel_ids.insert(panel.id().to_string());
            }
            handled |= keyboard_handled;
            actions.extend(panel_actions);
        }
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
        RuntimeKeyboardResult {
            handled,
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// 現在の値を ドキュメント subset へ変換する。
    fn sync_document_subset(
        &mut self,
        document: &Document,
        panel_ids: Option<&BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let mut changed_panels = BTreeSet::new();
        for panel in &mut self.panels {
            if panel_ids.is_some_and(|panel_ids| !panel_ids.contains(panel.id())) {
                continue;
            }
            let previous_tree = self
                .panel_tree_cache
                .get(panel.id())
                .cloned()
                .unwrap_or_else(|| panel.panel_tree());
            panel.update(document);
            let next_tree = panel.panel_tree();
            self.panel_tree_cache
                .insert(panel.id().to_string(), next_tree.clone());
            if next_tree != previous_tree {
                changed_panels.insert(panel.id().to_string());
            }
        }
        changed_panels
    }

    /// Loaded panels を削除する。
    fn remove_loaded_panels(&mut self) {
        if self.loaded_panel_ids.is_empty() {
            return;
        }
        self.panels.retain(|panel| {
            !self
                .loaded_panel_ids
                .iter()
                .any(|loaded_id| loaded_id == panel.id())
        });
        self.loaded_panel_ids.clear();
        self.rebuild_tree_cache();
    }

    /// Tree cache を再構築する。
    fn rebuild_tree_cache(&mut self) {
        self.panel_tree_cache.clear();
        for panel in &self.panels {
            self.panel_tree_cache
                .insert(panel.id().to_string(), panel.panel_tree());
        }
    }
}

/// イベント パネル ID を計算して返す。
fn event_panel_id(event: &PanelEvent) -> &str {
    match event {
        PanelEvent::Activate { panel_id, .. }
        | PanelEvent::SetValue { panel_id, .. }
        | PanelEvent::DragValue { panel_id, .. }
        | PanelEvent::SetText { panel_id, .. }
        | PanelEvent::Keyboard { panel_id, .. } => panel_id,
    }
}
