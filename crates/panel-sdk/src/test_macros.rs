//! パネルのエントリポイント呼び出しテストを宣言的に生成するマクロ (BL-150)。
//!
//! 12 パネルに同型コピペされていた `#[test] fn entrypoints_callable_on_native()`
//! (init / on_host_change / 各 handler を native でひと通り呼ぶスモーク) を 1 つの
//! 宣言マクロへ畳む受け皿。`panel_init` / `panel_handler` / `panel_on_host_change`
//! マクロは元の Rust 関数を呼び出し可能なまま残すため、native テストでは各
//! 関数を直接呼べる (export wrapper を経由しない)。
//!
//! 引数なし handler は `name()`、payload を取る handler は `name(arg)` のように
//! **呼び出し式そのもの** を列挙する。これにより i32 / typed payload いずれの
//! 引数規約でもパネル側がそのまま値を渡せる。
//!
//! 使用例:
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!     panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
//!         init(),
//!         on_host_change(),
//!         add_layer(),
//!         handle_layer_list(0),
//!     });
//! }
//! ```

/// パネルエントリポイントを native で順に呼ぶスモークテストを生成する (BL-150)。
///
/// `assert_entrypoints!(<test_fn_name> => { call1, call2, ... });` の形で、各 call は
/// パネルの entrypoint 関数への完全な呼び出し式 (引数込み) を書く。生成される
/// `#[test]` 関数は列挙順にすべての call を実行し、native ビルドで panic / 型不整合
/// なく呼べることを保証する。
#[macro_export]
macro_rules! assert_entrypoints {
    ($test_name:ident => { $($call:expr),* $(,)? }) => {
        #[test]
        fn $test_name() {
            $( $call; )*
        }
    };
}
