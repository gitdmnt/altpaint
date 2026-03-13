//! コマンド記述子を組み立てる薄いビルダー API を提供する。

use panel_schema::{CommandDescriptor, HandlerResult};
use serde_json::Value;

/// コマンド を計算して返す。
pub fn command(name: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        descriptor: CommandDescriptor::new(name),
    }
}

/// `CommandDescriptor` を段階的に構築する。
#[derive(Debug, Clone)]
pub struct CommandBuilder {
    descriptor: CommandDescriptor,
}

impl CommandBuilder {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::String(value.into()));
        self
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::Bool(value));
        self
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn color(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.string(key, value)
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn value(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.descriptor.payload.insert(key.into(), value.into());
        self
    }

    /// 構築 を計算して返す。
    pub fn build(self) -> CommandDescriptor {
        self.descriptor
    }
}

/// ハンドラ 結果 を計算して返す。
pub fn handler_result() -> HandlerResult {
    HandlerResult::default()
}
