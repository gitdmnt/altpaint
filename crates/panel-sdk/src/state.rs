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
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl IntKey {
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl StringKey {
    pub const fn new(path: &'static str) -> Self {
        Self(path)
    }
}

impl AsRef<str> for BoolKey {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for IntKey {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for StringKey {
    fn as_ref(&self) -> &str {
        self.0
    }
}

pub const fn bool(path: &'static str) -> BoolKey {
    BoolKey::new(path)
}

pub const fn int(path: &'static str) -> IntKey {
    IntKey::new(path)
}

pub const fn string(path: &'static str) -> StringKey {
    StringKey::new(path)
}
