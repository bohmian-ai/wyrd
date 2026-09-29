//! Proc-macro support for Wyrd error-code enums.

#![deny(missing_docs)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Lit, parse_macro_input};

/// Derive stable metadata accessors from `#[wyrd_error(...)]`.
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
    let mut title_arms = Vec::new();
    let mut remediation_arms = Vec::new();
    // Catalog enumeration: static variants push their code, delegates expand
    // their inner catalog, so generated projections see every reachable code.
    let mut code_list = Vec::new();
    // Reverse code->variant reconstruction, emitted only for variants whose
    // named fields are exactly `{ message, details }`. This is the inverse of
    // `code()`: it lets a transport boundary rebuild the typed variant (and so
    // its real `status()`) from a stable code, instead of collapsing every
    // unknown code into one catch-all status. Variants with any other shape
    // (unit, tuple, delegate, extra fields) do not qualify and fall through.
    let mut from_code_arms = Vec::new();
    let mut from_code_field_types: Option<(syn::Type, syn::Type)> = None;

    for variant in &data.variants {
        let metadata = match parse_attr(variant) {
            Ok(Some(metadata)) => metadata,
            Ok(None) => {
                return syn::Error::new_spanned(
                    variant,
                    "missing #[wyrd_error(code = \"WYRD_<DOMAIN>_<STATUS>_<SLUG>\", status = N, title = \"...\", remediation = \"...\")] or #[wyrd_error(delegate)]",
                )
                .to_compile_error()
                .into();
            }
            Err(error) => return error.to_compile_error().into(),
        };

        let ident = &variant.ident;
        match metadata {
            ErrorMetadata::Static {
                code,
                status,
                title,
                remediation,
            } => {
                if let Err(message) = validate_code(&code, status) {
                    return syn::Error::new_spanned(variant, message)
                        .to_compile_error()
                        .into();
                }
                let pattern = match &variant.fields {
                    Fields::Unit => quote! { #name::#ident },
                    Fields::Unnamed(_) => quote! { #name::#ident(..) },
                    Fields::Named(_) => quote! { #name::#ident { .. } },
                };
                code_arms.push(quote! { #pattern => #code });
                status_arms.push(quote! { #pattern => #status });
                title_arms.push(quote! { #pattern => #title });
                remediation_arms.push(quote! { #pattern => #remediation });
                code_list.push(quote! { codes.push(#code); });

                if let Some((message_ty, details_ty)) = message_details_fields(&variant.fields) {
                    // Pin the param types to the first qualifying variant and
                    // only include variants whose `message`/`details` types
                    // match, so the generated constructor always compiles.
                    let matches = match &from_code_field_types {
                        Some((m, d)) => types_equal(m, message_ty) && types_equal(d, details_ty),
                        None => {
                            from_code_field_types = Some((message_ty.clone(), details_ty.clone()));
                            true
                        }
                    };
                    if matches {
                        from_code_arms.push(quote! {
                            #code => ::core::option::Option::Some(
                                #name::#ident { message, details }
                            )
                        });
                    }
                }
            }
            ErrorMetadata::Delegate => {
                let (pattern, binding, inner_ty) =
                    match &variant.fields {
                        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => (
                            quote! { #name::#ident(inner) },
                            quote! { inner },
                            &fields.unnamed[0].ty,
                        ),
                        Fields::Named(fields) if fields.named.len() == 1 => {
                            let Some((field, ty)) = fields.named.first().and_then(|field| {
                                field.ident.as_ref().map(|ident| (ident, &field.ty))
                            }) else {
                                return syn::Error::new_spanned(
                                    variant,
                                    "#[wyrd_error(delegate)] named field must have an identifier",
                                )
                                .to_compile_error()
                                .into();
                            };
                            (quote! { #name::#ident { #field } }, quote! { #field }, ty)
                        }
                        _ => {
                            return syn::Error::new_spanned(
                            variant,
                            "#[wyrd_error(delegate)] requires exactly one named or unnamed field",
                        )
                        .to_compile_error()
                        .into();
                        }
                    };
                code_arms.push(quote! { #pattern => #binding.code() });
                status_arms.push(quote! { #pattern => #binding.status() });
                title_arms.push(quote! { #pattern => #binding.title() });
                remediation_arms.push(quote! { #pattern => #binding.remediation() });
                code_list.push(quote! { codes.extend(<#inner_ty>::codes()); });
            }
        }
    }

    let from_code_method = match from_code_field_types {
        Some((message_ty, details_ty)) => quote! {
            /// Reconstruct the typed variant for a stable Wyrd `code`.
            ///
            /// The inverse of [`Self::code`] for variants whose fields are
            /// exactly `{ message, details }`. Returns `None` for any code that
            /// has no such variant, so a caller can fall back to a catch-all
            /// while preserving the original code. Reconstructing the typed
            /// variant restores its real [`Self::status`] instead of collapsing
            /// every unrecognized code onto one status.
            pub fn from_code(
                code: &str,
                message: #message_ty,
                details: #details_ty,
            ) -> ::core::option::Option<Self> {
                match code {
                    #(#from_code_arms,)*
                    _ => ::core::option::Option::None,
                }
            }
        },
        None => quote! {},
    };

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

            /// Stable problem-title text.
            pub fn title(&self) -> &'static str {
                match self {
                    #(#title_arms,)*
                }
            }

            /// Operator-facing remediation hint.
            pub fn remediation(&self) -> &'static str {
                match self {
                    #(#remediation_arms,)*
                }
            }

            /// Every stable code [`Self::code`] can return, in declaration order.
            ///
            /// Static variants contribute their own code and delegate variants
            /// expand their inner catalog, so generated language projections
            /// (such as the TypeScript `WyrdErrorCode` union) enumerate the full
            /// reachable catalog from this one source.
            pub fn codes() -> ::std::vec::Vec<&'static str> {
                let mut codes = ::std::vec::Vec::new();
                #(#code_list)*
                codes
            }

            #from_code_method
        }
    }
    .into()
}

/// Return the `message` and `details` field types when `fields` is a named
/// struct whose fields are exactly `message` and `details`, else `None`.
fn message_details_fields(fields: &Fields) -> Option<(&syn::Type, &syn::Type)> {
    let Fields::Named(named) = fields else {
        return None;
    };
    if named.named.len() != 2 {
        return None;
    }
    let mut message_ty = None;
    let mut details_ty = None;
    for field in &named.named {
        match field.ident.as_ref()?.to_string().as_str() {
            "message" => message_ty = Some(&field.ty),
            "details" => details_ty = Some(&field.ty),
            _ => return None,
        }
    }
    Some((message_ty?, details_ty?))
}

/// Compare two field types for `from_code` eligibility.
///
/// A path type is compared by its final segment, so a variant written against
/// an imported alias (`details: Value`) qualifies alongside one written in
/// full (`details: serde_json::Value`). They name the same type, and a
/// mismatch that slipped through would fail to compile in the generated
/// constructor rather than pass silently. Any other type shape falls back to
/// token equality.
fn types_equal(left: &syn::Type, right: &syn::Type) -> bool {
    if let (syn::Type::Path(left), syn::Type::Path(right)) = (left, right)
        && let (Some(left), Some(right)) = (left.path.segments.last(), right.path.segments.last())
    {
        return quote!(#left).to_string() == quote!(#right).to_string();
    }
    quote!(#left).to_string() == quote!(#right).to_string()
}

enum ErrorMetadata {
    Static {
        code: String,
        status: u16,
        title: String,
        remediation: String,
    },
    Delegate,
}

fn parse_attr(variant: &syn::Variant) -> syn::Result<Option<ErrorMetadata>> {
    let Some(attr) = variant
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("wyrd_error"))
    else {
        return Ok(None);
    };
    let mut code = None;
    let mut status = None;
    let mut title = None;
    let mut remediation = None;
    let mut delegate = false;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("delegate") {
            delegate = true;
            return Ok(());
        }
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
        if meta.path.is_ident("title") {
            let value = meta.value()?;
            let Lit::Str(lit) = value.parse()? else {
                return Err(meta.error("title must be a string literal"));
            };
            title = Some(lit.value());
            return Ok(());
        }
        if meta.path.is_ident("remediation") {
            let value = meta.value()?;
            let Lit::Str(lit) = value.parse()? else {
                return Err(meta.error("remediation must be a string literal"));
            };
            remediation = Some(lit.value());
            return Ok(());
        }
        Err(meta.error("unsupported wyrd_error attribute key"))
    })?;

    if delegate {
        if code.is_some() || status.is_some() || title.is_some() || remediation.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "#[wyrd_error(delegate)] cannot be combined with code, status, title, or remediation",
            ));
        }
        return Ok(Some(ErrorMetadata::Delegate));
    }

    let (Some(code), Some(status), Some(title), Some(remediation)) =
        (code, status, title, remediation)
    else {
        return Ok(None);
    };

    Ok(Some(ErrorMetadata::Static {
        code,
        status,
        title,
        remediation,
    }))
}

fn validate_code(code: &str, status: u16) -> Result<(), String> {
    if !(100..=599).contains(&status) {
        return Err("status must be in 100..=599".to_string());
    }
    let parts: Vec<_> = code.split('_').collect();
    if parts.len() < 4 || parts.first() != Some(&"WYRD") {
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

#[cfg(test)]
mod tests {
    use super::parse_attr;

    #[test]
    fn delegate_metadata_rejects_static_fields() {
        let input: syn::DeriveInput = syn::parse_str(
            r#"
            enum Example {
                #[wyrd_error(
                    delegate,
                    code = "WYRD_TEST_500_INTERNAL",
                    status = 500,
                    title = "Internal failure",
                    remediation = "Retry later."
                )]
                Wrapped(Inner),
            }
            "#,
        )
        .expect("test enum parses");
        let syn::Data::Enum(data) = input.data else {
            panic!("test input is an enum");
        };
        let variant = data.variants.first().expect("test variant exists");

        let Err(error) = parse_attr(variant) else {
            panic!("delegate mixed with static metadata should fail");
        };

        assert!(
            error
                .to_string()
                .contains("cannot be combined with code, status, title, or remediation")
        );
    }
}
