//! The `#[register]` attribute for [impl-scan](https://github.com/GNP50/impl-scan).
//!
//! The attribute does nothing at compile time: it returns the item unchanged. Its
//! arguments are read from the source by `impl_scan::Scanner` in a build script and
//! passed to the templates as metadata. The macro only checks that the arguments are
//! well formed, so a typo fails where it is written instead of inside generated code.
//!
//! ```ignore
//! use impl_scan_attr::register;
//!
//! #[register(name = "MAX_FREE_SLOT", aliases = ["MFS"], reach_table = "Gnpy")]
//! impl Strategy for MaxFreeSlot { /* ... */ }
//! ```
//!
//! Accepted arguments are `key = value` pairs, where `value` is a string, integer,
//! float or boolean literal, or an array of them. `name` (a string) and `aliases`
//! (an array of strings) have a fixed meaning; every other key is free metadata.

use proc_macro::TokenStream;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprArray, ExprLit, Lit, MetaNameValue, Token};

/// Marks an `impl` block (or a type) for the impl-scan registry. See the crate docs.
#[proc_macro_attribute]
pub fn register(args: TokenStream, item: TokenStream) -> TokenStream {
    match check_args(args.into()) {
        Ok(()) => item,
        Err(err) => {
            let mut out: proc_macro2::TokenStream = err.to_compile_error();
            out.extend(proc_macro2::TokenStream::from(item));
            out.into()
        }
    }
}

fn check_args(args: proc_macro2::TokenStream) -> syn::Result<()> {
    let pairs = Punctuated::<MetaNameValue, Token![,]>::parse_terminated.parse2(args)?;
    let mut seen: Vec<String> = Vec::new();
    for pair in &pairs {
        let key = pair
            .path
            .get_ident()
            .ok_or_else(|| {
                syn::Error::new_spanned(&pair.path, "expected a plain key such as `name`")
            })?
            .to_string();
        if seen.contains(&key) {
            return Err(syn::Error::new_spanned(
                &pair.path,
                format!("duplicate key `{key}`"),
            ));
        }
        match key.as_str() {
            "name" => expect_str(&pair.value)?,
            "aliases" => match &pair.value {
                Expr::Array(ExprArray { elems, .. }) => {
                    for elem in elems {
                        expect_str(elem)?;
                    }
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "`aliases` must be an array of strings",
                    ))
                }
            },
            _ => expect_value(&pair.value)?,
        }
        seen.push(key);
    }
    Ok(())
}

fn expect_str(expr: &Expr) -> syn::Result<()> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(_), ..
        }) => Ok(()),
        other => Err(syn::Error::new_spanned(other, "expected a string literal")),
    }
}

fn expect_value(expr: &Expr) -> syn::Result<()> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(_) | Lit::Int(_) | Lit::Float(_) | Lit::Bool(_),
            ..
        }) => Ok(()),
        Expr::Array(ExprArray { elems, .. }) => elems.iter().try_for_each(expect_value),
        other => Err(syn::Error::new_spanned(
            other,
            "expected a literal (string, integer, float, bool) or an array of literals",
        )),
    }
}
