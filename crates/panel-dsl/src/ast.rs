//! `.altp-panel` DSL の構文木と正規化済み定義を保持する型群。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use thiserror::Error;

/// 解析直後の `.altp-panel` 全体 AST を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelAst {
    pub panel: PanelHeaderAst,
    pub permissions: Vec<String>,
    pub runtime: RuntimeAst,
    pub state: Vec<StateFieldAst>,
    pub view: Vec<ViewNodeAst>,
}

/// `panel {}` ブロックのヘッダー情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelHeaderAst {
    pub id: String,
    pub title: String,
    pub version: u32,
}

/// `runtime {}` ブロックの AST を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAst {
    pub wasm: String,
}

/// `state {}` ブロックの 1 フィールド定義を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldAst {
    pub name: String,
    pub kind: StateType,
    pub default: AttrValue,
}

/// DSL が許可する状態型を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateType {
    Bool,
    Int,
    Float,
    String,
    Color,
    Enum(Vec<String>),
}

/// 属性値のリテラルまたは式を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    String(String),
    Integer(i64),
    Float(String),
    Bool(bool),
    Expression(String),
}

impl AttrValue {
    /// 現在の値を string 形式へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// 現在の値を bool literal 形式へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn as_bool_literal(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

/// `view {}` ブロック中のノード AST を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNodeAst {
    Element(ViewElementAst),
    Text(String),
}

/// 解析直後の view 要素を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewElementAst {
    pub tag: String,
    pub attributes: BTreeMap<String, AttrValue>,
    pub children: Vec<ViewNodeAst>,
}

/// 検証と正規化を終えたパネル定義を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDefinition {
    pub source_path: PathBuf,
    pub manifest: PanelManifest,
    pub permissions: Vec<String>,
    pub runtime: RuntimeDefinition,
    pub state: Vec<StateField>,
    pub view: Vec<ViewNode>,
    pub handler_bindings: BTreeSet<String>,
}

/// 正規化済みパネルメタデータを表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelManifest {
    pub id: String,
    pub title: String,
    pub version: u32,
}

/// 正規化済みランタイム定義を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDefinition {
    pub wasm: String,
}

/// 正規化済み state フィールドを表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateField {
    pub name: String,
    pub kind: StateType,
    pub default: AttrValue,
}

/// 正規化済み view ノードを表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNode {
    Element(ViewElement),
    Text(String),
}

/// 正規化済み view 要素を表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewElement {
    pub tag: String,
    pub attributes: BTreeMap<String, AttrValue>,
    pub children: Vec<ViewNode>,
}

/// DSL の読み込みと検証で返す失敗種別を表す。
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PanelDslError {
    #[error("failed to read panel file: {0}")]
    Io(String),
    #[error("missing block: {0}")]
    MissingBlock(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(String),
}
