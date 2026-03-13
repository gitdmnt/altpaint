//! パネルローカル state パスを型として表す補助 API を提供する。

/// 真偽値 state キーを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoolKey(&'static str);

/// 整数 state キーを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntKey(&'static str);

/// 文字列 state キーを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringKey(&'static str);

impl BoolKey {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl IntKey {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl StringKey {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl AsRef<str> for BoolKey {
    /// 現在の値を ref 形式へ変換する。
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for IntKey {
    /// 現在の値を ref 形式へ変換する。
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for StringKey {
    /// 現在の値を ref 形式へ変換する。
    fn as_ref(&self) -> &str {
        self.0
    }
}

/// bool を計算して返す。
pub const fn bool(path: &'static str) -> BoolKey {
    BoolKey::new(path)
}

/// int を計算して返す。
pub const fn int(path: &'static str) -> IntKey {
    IntKey::new(path)
}

/// string を計算して返す。
pub const fn string(path: &'static str) -> StringKey {
    StringKey::new(path)
}
