//! Implementation of the FromAst derive macro
//!
//! Uses syn 2.x API for attribute parsing

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parse, parse::ParseStream, Data, DeriveInput, Expr, Fields, Ident, Lit, Token};

/// Custom attribute structure for parsanol attributes
#[derive(Debug, Default)]
struct ParsanolAttrs {
    rule: Option<String>,
    transparent: bool,
    tag: Option<String>,
    tag_expr: Option<Expr>,
    field: Option<String>,
    default: Option<DefaultKind>,
}

#[derive(Debug)]
enum DefaultKind {
    Simple,
    Expr(Expr),
}

mod kw {
    syn::custom_keyword!(parsanol);
    syn::custom_keyword!(rule);
    syn::custom_keyword!(transparent);
    syn::custom_keyword!(tag);
    syn::custom_keyword!(tag_expr);
    syn::custom_keyword!(field);
    syn::custom_keyword!(default);
}

/// Parse a single parsanol attribute
impl Parse for ParsanolAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attrs = ParsanolAttrs::default();

        while !input.is_empty() {
            let lookahead = input.lookahead1();
            if lookahead.peek(kw::rule) {
                input.parse::<kw::rule>()?;
                input.parse::<Token![=]>()?;
                let lit: Lit = input.parse()?;
                if let Lit::Str(s) = lit {
                    attrs.rule = Some(s.value());
                }
            } else if lookahead.peek(kw::transparent) {
                input.parse::<kw::transparent>()?;
                attrs.transparent = true;
            } else if lookahead.peek(kw::tag) {
                input.parse::<kw::tag>()?;
                input.parse::<Token![=]>()?;
                let lit: Lit = input.parse()?;
                if let Lit::Str(s) = lit {
                    attrs.tag = Some(s.value());
                }
            } else if lookahead.peek(kw::tag_expr) {
                input.parse::<kw::tag_expr>()?;
                input.parse::<Token![=]>()?;
                let expr: Expr = input.parse()?;
                attrs.tag_expr = Some(expr);
            } else if lookahead.peek(kw::field) {
                input.parse::<kw::field>()?;
                input.parse::<Token![=]>()?;
                let lit: Lit = input.parse()?;
                if let Lit::Str(s) = lit {
                    attrs.field = Some(s.value());
                }
            } else if lookahead.peek(kw::default) {
                input.parse::<kw::default>()?;
                if input.peek(Token![=]) {
                    input.parse::<Token![=]>()?;
                    let expr: Expr = input.parse()?;
                    attrs.default = Some(DefaultKind::Expr(expr));
                } else {
                    attrs.default = Some(DefaultKind::Simple);
                }
            } else {
                return Err(lookahead.error());
            }

            // Handle optional comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(attrs)
    }
}

/// Main implementation of the FromAst derive
pub fn derive_from_ast_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Parse container attributes
    let container_attrs = parse_attrs(&input.attrs)?;
    let transparent = container_attrs.transparent;

    let from_ast_impl = match &input.data {
        Data::Enum(data) => {
            if transparent {
                return Err(syn::Error::new_spanned(
                    input,
                    "transparent attribute is only valid for structs with a single field",
                ));
            }
            let variants: Vec<_> = data.variants.iter().collect();
            generate_enum_from_ast(name, &variants)?
        }
        Data::Struct(data) => generate_struct_from_ast(name, &data.fields, transparent)?,
        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "FromAst cannot be derived for unions",
            ));
        }
    };

    Ok(quote! {
        impl #impl_generics ::std::convert::TryFrom<parsanol::portable::AstNode>
            for #name #ty_generics #where_clause
        {
            type Error = parsanol::derive::FromAstError;

            fn try_from(node: parsanol::portable::AstNode) -> Result<Self, Self::Error> {
                #from_ast_impl
            }
        }
    })
}

/// Parse parsanol attributes from a list of attributes
fn parse_attrs(attrs: &[syn::Attribute]) -> syn::Result<ParsanolAttrs> {
    let mut result = ParsanolAttrs::default();

    for attr in attrs {
        if attr.path().is_ident("parsanol") {
            let parsed: ParsanolAttrs = attr.parse_args()?;
            if let Some(rule) = parsed.rule {
                result.rule = Some(rule);
            }
            if parsed.transparent {
                result.transparent = true;
            }
            if let Some(tag) = parsed.tag {
                result.tag = Some(tag);
            }
            if let Some(tag_expr) = parsed.tag_expr {
                result.tag_expr = Some(tag_expr);
            }
            if let Some(field) = parsed.field {
                result.field = Some(field);
            }
            if let Some(default) = parsed.default {
                result.default = Some(default);
            }
        }
    }

    Ok(result)
}

/// Generate FromAst implementation for enums
fn generate_enum_from_ast(name: &Ident, variants: &[&syn::Variant]) -> syn::Result<TokenStream> {
    let mut match_arms = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;
        let attrs = parse_attrs(&variant.attrs)?;

        // Get the tag attribute
        let tag = attrs.tag;
        let tag_expr = attrs.tag_expr;

        // Determine how to match
        let matcher = if let Some(tag) = tag {
            quote! { Some(#tag) }
        } else if let Some(tag_expr) = tag_expr {
            quote! { Some(#tag_expr) }
        } else {
            return Err(syn::Error::new_spanned(
                variant,
                "enum variants must have #[parsanol(tag = \"...\")] or #[parsanol(tag_expr = ...)]",
            ));
        };

        // Generate the conversion for this variant
        let conversion = match &variant.fields {
            Fields::Named(fields) => {
                let field_conversions: Vec<TokenStream> = fields
                    .named
                    .iter()
                    .map(|f| generate_field_extraction(f))
                    .collect::<syn::Result<Vec<_>>>()?;

                let field_names: Vec<&Ident> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().unwrap())
                    .collect();

                quote! {
                    #(#field_conversions)*
                    #name::#variant_name {
                        #(#field_names),*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                if fields.unnamed.is_empty() {
                    quote! { #name::#variant_name }
                } else if fields.unnamed.len() == 1 {
                    quote! {
                        #name::#variant_name(
                            ::std::convert::TryInto::try_into(node)
                                .map_err(|_| parsanol::derive::FromAstError::ConversionError)?
                        )
                    }
                } else {
                    // Multi-field tuple variant
                    let indices: Vec<usize> = (0..fields.unnamed.len()).collect();
                    quote! {
                        #name::#variant_name(
                            #(
                                arr.get(#indices)
                                    .and_then(|v| ::std::convert::TryInto::try_into(*v).ok())
                                    .ok_or(parsanol::derive::FromAstError::ConversionError)?
                            ),*
                        )
                    }
                }
            }
            Fields::Unit => {
                quote! { #name::#variant_name }
            }
        };

        match_arms.push(quote! {
            #matcher => { #conversion }
        });
    }

    Ok(quote! {
        let tag: Option<&str> = None; // TODO: Extract tag from hash
        match tag.map(|s| s.to_string()) {
            #(#match_arms)*
            _ => Err(parsanol::derive::FromAstError::UnknownTag),
        }
    })
}

/// Generate FromAst implementation for structs
fn generate_struct_from_ast(
    name: &Ident,
    fields: &Fields,
    transparent: bool,
) -> syn::Result<TokenStream> {
    match fields {
        Fields::Named(fields) => {
            let field_conversions: Vec<TokenStream> = fields
                .named
                .iter()
                .map(|f| generate_field_extraction(f))
                .collect::<syn::Result<Vec<_>>>()?;

            let field_names: Vec<&Ident> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();

            Ok(quote! {
                #(#field_conversions)*
                Ok(#name {
                    #(#field_names),*
                })
            })
        }
        Fields::Unnamed(fields) => {
            if fields.unnamed.is_empty() {
                return Ok(quote! { Ok(#name) });
            }

            if fields.unnamed.len() == 1 && transparent {
                return Ok(quote! {
                    Ok(#name(
                        ::std::convert::TryInto::try_into(node)
                            .map_err(|_| parsanol::derive::FromAstError::ConversionError)?
                    ))
                });
            }

            // Extract from array
            let conversions: Vec<TokenStream> = fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    quote! {
                        arr.get(#i)
                            .and_then(|v| ::std::convert::TryInto::try_into(*v).ok())
                            .ok_or(parsanol::derive::FromAstError::ConversionError)?
                    }
                })
                .collect();

            Ok(quote! {
                let arr: &[parsanol::portable::AstNode] = match node {
                    parsanol::portable::AstNode::Array { .. } => &[],
                    _ => return Err(parsanol::derive::FromAstError::ExpectedArray),
                };
                Ok(#name(#(#conversions),*))
            })
        }
        Fields::Unit => Ok(quote! { Ok(#name) }),
    }
}

/// Generate field extraction code
fn generate_field_extraction(field: &syn::Field) -> syn::Result<TokenStream> {
    let fname = field.ident.as_ref().unwrap();
    let attrs = parse_attrs(&field.attrs)?;
    let _field_name = attrs.field.unwrap_or_else(|| fname.to_string());
    let field_ty = &field.ty;

    let extract = match attrs.default {
        Some(DefaultKind::Simple) => {
            quote! {
                let #fname: #field_ty = ::std::convert::TryInto::try_into(node)
                    .unwrap_or_default();
            }
        }
        Some(DefaultKind::Expr(expr)) => {
            quote! {
                let #fname: #field_ty = ::std::convert::TryInto::try_into(node)
                    .unwrap_or_else(|_| #expr);
            }
        }
        None => {
            quote! {
                let #fname: #field_ty = ::std::convert::TryInto::try_into(node)
                    .map_err(|_| parsanol::derive::FromAstError::ConversionError)?;
            }
        }
    };

    Ok(extract)
}
