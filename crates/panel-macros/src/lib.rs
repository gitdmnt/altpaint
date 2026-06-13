use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    FnArg, Ident, ItemFn, LitStr, Pat, ReturnType, Type, parse::Parse, parse::ParseStream,
    parse_macro_input,
};

struct HandlerArgs {
    export_name: Option<LitStr>,
}

impl Parse for HandlerArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { export_name: None });
        }

        let key: Ident = input.parse()?;
        if key != "name" {
            return Err(syn::Error::new(key.span(), "expected `name = \"...\"`"));
        }
        input.parse::<syn::Token![=]>()?;
        let export_name: LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after handler export name"));
        }
        Ok(Self {
            export_name: Some(export_name),
        })
    }
}

#[proc_macro_attribute]
pub fn panel_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`panel_init` does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let function = parse_macro_input!(item as ItemFn);
    expand_panel_export(function, "panel_init", true, None)
}

#[proc_macro_attribute]
pub fn panel_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as HandlerArgs);
    let function = parse_macro_input!(item as ItemFn);
    expand_panel_export(function, "panel_handle", false, args.export_name)
}

#[proc_macro_attribute]
pub fn panel_sync_host(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`panel_sync_host` does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let function = parse_macro_input!(item as ItemFn);
    expand_panel_export(
        function,
        "panel_sync_host",
        true,
        Some(LitStr::new(
            "panel_sync_host",
            proc_macro2::Span::call_site(),
        )),
    )
}

/// handler 引数の種別 (BL-141 handler payload 規約)。
enum HandlerArgKind {
    /// 引数なし。
    None,
    /// legacy `i32` payload (`event_payload["value"]`)。移行期間のみ。
    LegacyI32,
    /// typed payload (`T: serde::Deserialize + Default`)。`event_payload` 全体を渡す。
    Typed(Box<Type>),
}

fn expand_panel_export(
    function: ItemFn,
    export_prefix: &str,
    is_init: bool,
    explicit_name: Option<LitStr>,
) -> TokenStream {
    let arg_kind = match validate_signature(&function.sig, is_init) {
        Ok(arg_kind) => arg_kind,
        Err(error) => return error.to_compile_error().into(),
    };

    let attrs = &function.attrs;
    let vis = &function.vis;
    let sig = &function.sig;
    let block = &function.block;
    let function_name = &sig.ident;
    let wrapper_name = format_ident!("__altpaint_export_{}", function_name);
    let export_name = explicit_name.unwrap_or_else(|| {
        if is_init {
            LitStr::new("panel_init", function_name.span())
        } else {
            LitStr::new(
                &format!("{export_prefix}_{}", function_name),
                function_name.span(),
            )
        }
    });

    // 種別ごとに wrapper の FFI シグネチャと handler 呼出を組み立てる。
    // typed payload は FFI 引数を持たず、wrapper 内で event_payload を取得する。
    let (wrapper_params, call) = match arg_kind {
        HandlerArgKind::None => (quote!(), quote!(#function_name();)),
        HandlerArgKind::LegacyI32 => (
            quote!(value: i32),
            quote!(#function_name(value);),
        ),
        HandlerArgKind::Typed(ty) => (
            quote!(),
            quote!(
                let payload: #ty = ::panel_sdk::runtime::event_payload::<#ty>();
                #function_name(payload);
            ),
        ),
    };

    quote!(
        #(#attrs)*
        #vis #sig #block

        #[doc(hidden)]
        #[unsafe(export_name = #export_name)]
        pub extern "C" fn #wrapper_name(#wrapper_params) {
            #call
        }
    )
    .into()
}

fn validate_signature(signature: &syn::Signature, is_init: bool) -> syn::Result<HandlerArgKind> {
    if signature.constness.is_some() {
        return Err(syn::Error::new(
            signature.constness.span(),
            "panel entrypoints cannot be const",
        ));
    }
    if signature.asyncness.is_some() {
        return Err(syn::Error::new(
            signature.asyncness.span(),
            "panel entrypoints cannot be async",
        ));
    }
    if !signature.generics.params.is_empty() {
        return Err(syn::Error::new(
            signature.generics.span(),
            "panel entrypoints cannot be generic",
        ));
    }
    if !matches!(signature.output, ReturnType::Default) {
        return Err(syn::Error::new(
            signature.output.span(),
            "panel entrypoints must return `()`",
        ));
    }
    if signature.abi.is_some() {
        return Err(syn::Error::new(
            signature.abi.span(),
            "panel entrypoints should be plain Rust functions; the SDK exports the extern wrapper",
        ));
    }

    let input_count = signature.inputs.len();
    if is_init && input_count != 0 {
        return Err(syn::Error::new(
            signature.inputs.span(),
            "`panel_init` functions cannot take arguments",
        ));
    }
    if !is_init && input_count > 1 {
        return Err(syn::Error::new(
            signature.inputs.span(),
            "panel handlers take zero or one payload argument (typed serde struct or legacy i32)",
        ));
    }

    let Some(argument) = signature.inputs.first() else {
        return Ok(HandlerArgKind::None);
    };
    match argument {
        // legacy i32 payload は移行期間のみ許可 (typed payload への移行で撤去)。
        FnArg::Typed(argument) if matches_i32(&argument.ty) => Ok(HandlerArgKind::LegacyI32),
        // それ以外の単一型引数は typed payload (T: Deserialize + Default) とみなす。
        FnArg::Typed(argument) => {
            if !matches!(&*argument.pat, Pat::Ident(_)) {
                return Err(syn::Error::new(
                    argument.pat.span(),
                    "panel handler payload argument must be a simple identifier",
                ));
            }
            Ok(HandlerArgKind::Typed(argument.ty.clone()))
        }
        FnArg::Receiver(receiver) => Err(syn::Error::new(
            receiver.span(),
            "panel entrypoints cannot take `self`",
        )),
    }
}

fn matches_i32(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "i32"),
        _ => false,
    }
}
