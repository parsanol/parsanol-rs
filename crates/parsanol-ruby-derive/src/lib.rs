//! Proc macro crate for deriving RubyObject trait
//!
//! This crate provides the `#[derive(RubyObject)]` macro for automatically
//! implementing the `RubyObject` trait, which enables direct Ruby object
//! construction from Rust types (Native).
//!
//! # Example
//!
//! ```rust,ignore
//! use parsanol_ruby_derive::RubyObject;
//!
//! #[derive(Debug, Clone, RubyObject)]
//! #[ruby_class("Calculator::Expr")]
//! pub enum Expr {
//!     #[ruby_variant("number")]
//!     Number(i64),
//!
//!     #[ruby_variant("binop")]
//!     BinOp {
//!         left: Box<Expr>,
//!         op: String,
//!         right: Box<Expr>,
//!     },
//! }
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, Lit};

/// Derive macro for RubyObject trait
///
/// This macro generates an implementation of `RubyObject` that constructs
/// Ruby objects directly via FFI (using magnus).
///
/// # Container Attributes
///
/// - `#[ruby_class("MyModule::MyClass")]` - Specify the Ruby class name
///
/// # Variant Attributes
///
/// - `#[ruby_variant("variant_name")]` - Specify the Ruby variant/class name for this variant
///
/// # Field Attributes
///
/// - `#[ruby_attr("@field_name")]` - Specify the Ruby instance variable name
#[proc_macro_derive(RubyObject, attributes(ruby_class, ruby_variant, ruby_attr))]
pub fn derive_ruby_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    impl_ruby_object(&input).into()
}

fn impl_ruby_object(input: &DeriveInput) -> TokenStream2 {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Get the Ruby class name from attributes
    let ruby_class = get_string_attr(&input.attrs, "ruby_class")
        .unwrap_or_else(|| format!("Parsanol::{}", name));

    let impl_body = match &input.data {
        Data::Enum(data) => {
            let match_arms: Vec<_> = data.variants.iter().map(|variant| {
                let variant_name = &variant.ident;

                // Get the Ruby variant/class name
                let ruby_variant = get_string_attr(&variant.attrs, "ruby_variant")
                    .unwrap_or_else(|| variant_name.to_string().to_lowercase());

                // Full class path
                let class_path = if ruby_variant.contains("::") {
                    ruby_variant.clone()
                } else {
                    format!("{}::{}", ruby_class, to_pascal_case(&ruby_variant))
                };

                match &variant.fields {
                    Fields::Unit => {
                        quote! {
                            #name::#variant_name => {
                                let class: magnus::RClass = ruby.class(#class_path)?;
                                class.new_instance(())
                            }
                        }
                    }
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                        quote! {
                            #name::#variant_name(inner) => {
                                let class: magnus::RClass = ruby.class(#class_path)?;
                                class.new_instance((inner,))
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let indices: Vec<_> = (0..fields.unnamed.len())
                            .map(|i| syn::Index::from(i))
                            .collect();
                        let bindings: Vec<_> = indices.iter()
                            .map(|_| quote::format_ident!("field"))
                            .collect();

                        quote! {
                            #name::#variant_name(#(#bindings),*) => {
                                let class: magnus::RClass = ruby.class(#class_path)?;
                                class.new_instance((#(#bindings,)*))
                            }
                        }
                    }
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields.named.iter()
                            .map(|f| f.ident.as_ref().unwrap())
                            .collect();
                        let field_attrs: Vec<_> = fields.named.iter()
                            .map(|f| {
                                get_string_attr(&f.attrs, "ruby_attr")
                                    .unwrap_or_else(|| format!("@{}", f.ident.as_ref().unwrap()))
                            })
                            .collect();

                        quote! {
                            #name::#variant_name { #(#field_names),* } => {
                                let class: magnus::RClass = ruby.class(#class_path)?;
                                let obj: magnus::RObject = class.new_instance()?;
                                #(
                                    obj.ivar_set(#field_attrs,
                                        parsanol::ruby_ffi::RubyObject::to_ruby(#field_names, ruby)?)?;
                                )*
                                Ok(obj.as_value())
                            }
                        }
                    }
                }
            }).collect();

            quote! {
                match self {
                    #(#match_arms)*
                }
            }
        }
        Data::Struct(data) => match &data.fields {
            Fields::Unit => {
                quote! {
                    let class: magnus::RClass = ruby.class(#ruby_class)?;
                    class.new_instance(())
                }
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                quote! {
                    let class: magnus::RClass = ruby.class(#ruby_class)?;
                    class.new_instance((self.0,))
                }
            }
            Fields::Unnamed(fields) => {
                let indices: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| syn::Index::from(i))
                    .collect();

                quote! {
                    let class: magnus::RClass = ruby.class(#ruby_class)?;
                    class.new_instance((#(self.#indices,)*))
                }
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().unwrap())
                    .collect();
                let field_attrs: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| {
                        get_string_attr(&f.attrs, "ruby_attr")
                            .unwrap_or_else(|| format!("@{}", f.ident.as_ref().unwrap()))
                    })
                    .collect();

                quote! {
                    let class: magnus::RClass = ruby.class(#ruby_class)?;
                    let obj: magnus::RObject = class.new_instance()?;
                    #(
                        obj.ivar_set(#field_attrs,
                            parsanol::ruby_ffi::RubyObject::to_ruby(&self.#field_names, ruby)?)?;
                    )*
                    Ok(obj.as_value())
                }
            }
        },
        Data::Union(_) => {
            return quote! {
                compile_error!("RubyObject can only be derived for enums and structs");
            };
        }
    };

    quote! {
        impl #impl_generics parsanol::ruby_ffi::RubyObject for #name #ty_generics #where_clause {
            fn to_ruby(&self, ruby: &magnus::Ruby) -> Result<magnus::Value, magnus::Error> {
                #impl_body
            }
        }
    }
}

/// Get a string attribute value from attributes
fn get_string_attr(attrs: &[syn::Attribute], attr_name: &str) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            // Handle #[attr_name = "value"]
            if let Ok(expr) = attr.meta.require_name_value() {
                if let Expr::Lit(expr_lit) = &expr.value {
                    if let Lit::Str(s) = &expr_lit.lit {
                        return Some(s.value());
                    }
                }
            }
            // Handle #[attr_name("value")]
            if let Ok(lit) = attr.parse_args::<Lit>() {
                if let Lit::Str(str_lit) = lit {
                    return Some(str_lit.value());
                }
            }
        }
    }
    None
}

/// Convert string to PascalCase
fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("hello"), "Hello");
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("foo-bar-baz"), "FooBarBaz");
        assert_eq!(to_pascal_case("number"), "Number");
        assert_eq!(to_pascal_case("binop"), "Binop");
    }
}
