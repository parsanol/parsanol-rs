//! Derive macros for parsanol
//!
//! This crate provides derive macros for converting parsed AST nodes into
//! typed Rust structures.
//!
//! # Available Macros
//!
//! ## `FromAst`
//!
//! Converts an `AstNode` into a typed struct or enum.
//!
//! # Example
//!
//! ```rust,ignore
//! use parsanol::derive::FromAst;
//! use parsanol::portable::AstNode;
//!
//! #[derive(FromAst)]
//! #[parsanol(rule = "expression")]
//! pub enum Expr {
//!     #[parsanol(tag = "number")]
//!     Number(i64),
//!
//!     #[parsanol(tag = "string")]
//!     String(String),
//!
//!     #[parsanol(tag = "binop")]
//!     BinOp {
//!         left: Box<Expr>,
//!         op: String,
//!         right: Box<Expr>,
//!     },
//! }
//! ```

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod from_ast;

/// Derive macro for converting AstNode to typed structures
///
/// # Attributes
///
/// ## Container attributes
///
/// - `#[parsanol(rule = "name")]` - The grammar rule this type corresponds to
/// - `#[parsanol(transparent)]` - Directly convert without wrapper
///
/// ## Variant attributes
///
/// - `#[parsanol(tag = "name")]` - Match when the AST has this tag/name
/// - `#[parsanol(tag_expr = "pattern")]` - Match using a pattern
///
/// ## Field attributes
///
/// - `#[parsanol(field = "name")]` - Extract from hash field with this name
/// - `#[parsanol(default)]` - Use Default if field is missing
/// - `#[parsanol(default = "expr")]` - Use expression if field is missing
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::derive::FromAst;
///
/// #[derive(FromAst)]
/// #[parsanol(rule = "statement")]
/// pub enum Statement {
///     #[parsanol(tag = "let")]
///     Let {
///         #[parsanol(field = "name")]
///         name: String,
///         #[parsanol(field = "value")]
///         value: Expr,
///     },
///
///     #[parsanol(tag = "return")]
///     Return(Option<Expr>),
/// }
/// ```
#[proc_macro_derive(FromAst, attributes(parsanol))]
pub fn derive_from_ast(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    from_ast::derive_from_ast_impl(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
