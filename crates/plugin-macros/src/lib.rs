use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    FnArg, Ident, ItemFn, LitStr, Pat, PatIdent, ReturnType, Type, parse::Parse,
    parse::ParseStream, parse_macro_input,
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
        Some(LitStr::new("panel_sync_host", proc_macro2::Span::call_site())),
    )
}

fn expand_panel_export(
    function: ItemFn,
    export_prefix: &str,
    is_init: bool,
    explicit_name: Option<LitStr>,
) -> TokenStream {
    if let Err(error) = validate_signature(&function.sig, is_init) {
        return error.to_compile_error().into();
    }

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

    let wrapper_inputs = &sig.inputs;
    let call_arguments = sig.inputs.iter().map(call_argument_for_input);

    quote!(
        #(#attrs)*
        #vis #sig #block

        #[doc(hidden)]
        #[unsafe(export_name = #export_name)]
        pub extern "C" fn #wrapper_name(#wrapper_inputs) {
            #function_name(#(#call_arguments),*);
        }
    )
    .into()
}

fn validate_signature(signature: &syn::Signature, is_init: bool) -> syn::Result<()> {
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
            "panel handlers currently support zero or one `i32` argument",
        ));
    }
    if let Some(argument) = signature.inputs.first() {
        match argument {
            FnArg::Typed(argument) if matches_i32(&argument.ty) => {}
            FnArg::Typed(argument) => {
                return Err(syn::Error::new(
                    argument.ty.span(),
                    "panel handlers currently support only `i32` payload arguments",
                ));
            }
            FnArg::Receiver(receiver) => {
                return Err(syn::Error::new(
                    receiver.span(),
                    "panel entrypoints cannot take `self`",
                ));
            }
        }
    }

    Ok(())
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

fn call_argument_for_input(argument: &FnArg) -> proc_macro2::TokenStream {
    match argument {
        FnArg::Typed(argument) => match &*argument.pat {
            Pat::Ident(PatIdent { ident, .. }) => quote!(#ident),
            _ => quote!(compile_error!(
                "panel entrypoint arguments must be simple identifiers"
            )),
        },
        FnArg::Receiver(_) => quote!(compile_error!("panel entrypoints cannot take self")),
    }
}
