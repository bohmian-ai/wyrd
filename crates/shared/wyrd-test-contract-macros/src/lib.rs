//! Attribute macros for declaring test-critical Wyrd contracts.
//!
//! These macros intentionally leave the annotated Rust item unchanged. The
//! repository test-contract checker reads the attributes from source and
//! compares them to Python test coverage markers.

use proc_macro::TokenStream;
use quote::quote;
use syn::LitStr;

/// Mark a Rust item as requiring explicit test coverage.
///
/// The argument must be a stable contract id such as
/// `"python:DataCard.save"`. CI checks these ids against
/// `pytest.mark.wyrd_covers(...)` markers.
#[proc_macro_attribute]
pub fn critical(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_tokens = proc_macro2::TokenStream::from(item.clone());
    match syn::parse::<LitStr>(attr) {
        Ok(value) if !value.value().trim().is_empty() => item,
        Ok(_) => quote! {
            compile_error!("wyrd test contract id must not be empty");
            #item_tokens
        }
        .into(),
        Err(error) => {
            let message = format!("expected #[critical(\"surface:Owner.item\")]: {error}");
            quote! {
                compile_error!(#message);
                #item_tokens
            }
            .into()
        }
    }
}
