//! Proc-macro support for Wyrd error-code enums.

#![deny(missing_docs)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Lit, parse_macro_input};

/// Derive stable `code()` and `status()` accessors from `#[wyrd_error(...)]`.
#[proc_macro_derive(WyrdError, attributes(wyrd_error))]
pub fn derive_wyrd_error(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let Data::Enum(data) = &ast.data else {
        return syn::Error::new_spanned(&ast.ident, "WyrdError requires an enum")
            .to_compile_error()
            .into();
    };

    let mut code_arms = Vec::new();
    let mut status_arms = Vec::new();

    for variant in &data.variants {
        let Some((code, status)) = parse_attr(variant) else {
            return syn::Error::new_spanned(
                variant,
                "missing #[wyrd_error(code = \"WYRD_<DOMAIN>_<STATUS>_<SLUG>\", status = N)]",
            )
            .to_compile_error()
            .into();
        };
        if let Err(message) = validate_code(&code, status) {
            return syn::Error::new_spanned(variant, message)
                .to_compile_error()
                .into();
        }

        let ident = &variant.ident;
        let pattern = match &variant.fields {
            Fields::Unit => quote! { #name::#ident },
            Fields::Unnamed(_) => quote! { #name::#ident(..) },
            Fields::Named(_) => quote! { #name::#ident { .. } },
        };
        code_arms.push(quote! { #pattern => #code });
        status_arms.push(quote! { #pattern => #status });
    }

    quote! {
        impl #name {
            /// Stable Wyrd error code.
            pub fn code(&self) -> &'static str {
                match self {
                    #(#code_arms,)*
                }
            }

            /// Suggested HTTP status.
            pub fn status(&self) -> u16 {
                match self {
                    #(#status_arms,)*
                }
            }
        }
    }
    .into()
}

fn parse_attr(variant: &syn::Variant) -> Option<(String, u16)> {
    let attr = variant
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("wyrd_error"))?;
    let mut code = None;
    let mut status = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("code") {
            let value = meta.value()?;
            let Lit::Str(lit) = value.parse()? else {
                return Err(meta.error("code must be a string literal"));
            };
            code = Some(lit.value());
            return Ok(());
        }
        if meta.path.is_ident("status") {
            let value = meta.value()?;
            let Lit::Int(lit) = value.parse()? else {
                return Err(meta.error("status must be an integer literal"));
            };
            status = Some(lit.base10_parse::<u16>()?);
            return Ok(());
        }
        Err(meta.error("unsupported wyrd_error attribute key"))
    })
    .ok()?;

    Some((code?, status?))
}

fn validate_code(code: &str, status: u16) -> Result<(), String> {
    if !(100..=599).contains(&status) {
        return Err("status must be in 100..=599".to_string());
    }
    let parts: Vec<_> = code.split('_').collect();
    if parts.len() < 5 || parts.first() != Some(&"WYRD") {
        return Err("code must be WYRD_<DOMAIN>_<STATUS>_<SLUG>".to_string());
    }
    let code_status = parts[2]
        .parse::<u16>()
        .map_err(|_| "code status segment must be numeric".to_string())?;
    if code_status != status {
        return Err("code status segment must match status attribute".to_string());
    }
    if parts.iter().any(|part| part.is_empty()) {
        return Err("code segments must not be empty".to_string());
    }
    Ok(())
}
