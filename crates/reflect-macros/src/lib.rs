//! Automatic registration of structs and inherent methods in an inline module.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, FnArg, ImplItem, Item, ItemMod, Type, spanned::Spanned};

mod action_macro;

/// Generates an owned request schema and a dispatch adapter for an action function.
/// Mutable arenas are injected by argument name; mutable items become ObjectRef fields.
#[proc_macro_attribute]
pub fn action(attribute: TokenStream, input: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "action takes no arguments")
            .into_compile_error()
            .into();
    }
    let function = syn::parse_macro_input!(input as syn::ItemFn);
    match action_macro::expand(function) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Discovers named struct fields and methods in ordinary inherent impl blocks.
#[proc_macro_attribute]
pub fn reflect(attribute: TokenStream, input: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "reflect takes no arguments")
            .into_compile_error()
            .into();
    }
    let mut module = syn::parse_macro_input!(input as ItemMod);
    match expand(&mut module) {
        Ok(()) => quote!(#module).into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand(module: &mut ItemMod) -> syn::Result<()> {
    let (_, items) = module
        .content
        .as_mut()
        .ok_or_else(|| syn::Error::new(module.ident.span(), "reflect requires an inline module"))?;
    let mut generated = Vec::new();
    for item in items.iter() {
        let Item::Struct(structure) = item else {
            continue;
        };
        if !structure.generics.params.is_empty() || structure.generics.where_clause.is_some() {
            return Err(syn::Error::new(
                structure.span(),
                "reflected structs cannot be generic",
            ));
        }
        let name = &structure.ident;
        let fields = match &structure.fields {
            Fields::Named(fields) => fields.named.iter().map(|field| {
                let name = field.ident.as_ref().unwrap();
                let ty = &field.ty;
                let attrs = conditional_attributes(&field.attrs);
                if let Type::Path(path) = &field.ty
                    && let Some(segment) = path.path.segments.last().filter(|segment| segment.ident == "Vec")
                        && let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments
                            && let Some(syn::GenericArgument::Type(element)) = arguments.args.first() {
                                return quote!(#(#attrs)* ::pixui_reflect::Field::sequence::<Self, #element>(stringify!(#name), |receiver| &receiver.#name));
                            }
                quote!(#(#attrs)* ::pixui_reflect::Field::typed::<Self, #ty>(stringify!(#name), |receiver| &receiver.#name))
            }).collect::<Vec<_>>(),
            Fields::Unit => Vec::new(),
            Fields::Unnamed(_) => return Err(syn::Error::new(structure.span(), "reflect supports named or unit structs")),
        };
        let mut constructor_fields = Vec::new();
        if let Fields::Named(fields) = &structure.fields {
            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();
                let ty = &field.ty;
                let attrs = conditional_attributes(&field.attrs);
                constructor_fields.push(quote!(
                    #(#attrs)* #field_name: {
                        let index = field_index;
                        field_index += 1;
                        values.next().expect("field count validated")
                            .into_owned::<#ty>()
                            .map_err(|_| ::pixui_reflect::construction::field_error::<#ty>(index, stringify!(#field_name)))?
                    }
                ));
            }
        }
        let construct_value = if matches!(structure.fields, Fields::Unit) {
            quote!(Self)
        } else {
            quote!(Self { #(#constructor_fields),* })
        };
        let mut methods = Vec::new();
        for item in items.iter() {
            let Item::Impl(block) = item else { continue };
            if block.trait_.is_some() {
                continue;
            }
            let Type::Path(path) = block.self_ty.as_ref() else {
                continue;
            };
            if path.qself.is_some()
                || path.path.segments.len() != 1
                || path.path.segments[0].ident != *name
            {
                continue;
            }
            if !block.generics.params.is_empty() || block.generics.where_clause.is_some() {
                return Err(syn::Error::new(
                    block.span(),
                    "reflected impl blocks cannot be generic",
                ));
            }
            for member in &block.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                let signature = &method.sig;
                let Some(receiver) = signature.receiver() else {
                    continue;
                };
                if receiver.reference.is_none()
                    || receiver.colon_token.is_some()
                    || signature.asyncness.is_some()
                    || signature.unsafety.is_some()
                    || signature.abi.is_some()
                    || signature.variadic.is_some()
                    || !signature.generics.params.is_empty()
                    || signature.generics.where_clause.is_some()
                {
                    return Err(syn::Error::new(
                        signature.span(),
                        "reflected methods must be safe, synchronous, non-generic methods with &self or &mut self",
                    ));
                }
                let method_name = &signature.ident;
                let mut bindings = Vec::new();
                let mut arguments = Vec::new();
                for (index, input) in signature.inputs.iter().skip(1).enumerate() {
                    let FnArg::Typed(input) = input else {
                        unreachable!()
                    };
                    let argument = format_ident!("argument_{index}");
                    let ty = &input.ty;
                    let binding = match ty.as_ref() {
                        Type::Reference(reference) => {
                            if reference.mutability.is_some() || reference.lifetime.is_some() {
                                return Err(syn::Error::new(
                                    ty.span(),
                                    "reflected borrowed arguments must be shared references with elided lifetimes",
                                ));
                            }
                            let target = &reference.elem;
                            if matches!(target.as_ref(), Type::Path(p) if p.path.is_ident("str")) {
                                quote!(let #argument = ::pixui_reflect::argument::<::std::string::String>(arguments, #index)?.as_str();)
                            } else {
                                quote!(let #argument = ::pixui_reflect::argument::<#target>(arguments, #index)?;)
                            }
                        }
                        _ => {
                            quote!(let #argument = <#ty as ::core::clone::Clone>::clone(::pixui_reflect::argument::<#ty>(arguments, #index)?);)
                        }
                    };
                    bindings.push(binding);
                    arguments.push(argument);
                }
                let arity = arguments.len();
                let attrs = conditional_attributes(&method.attrs);
                let block_attrs = conditional_attributes(&block.attrs);
                let (constructor, result) = match &signature.output {
                    syn::ReturnType::Type(_, ty) if matches!(ty.as_ref(), Type::Reference(_)) => {
                        let Type::Reference(reference) = ty.as_ref() else {
                            unreachable!()
                        };
                        if reference.lifetime.is_some() {
                            return Err(syn::Error::new(
                                ty.span(),
                                "reflected return references must use the receiver's elided lifetime",
                            ));
                        }
                        if reference.mutability.is_some() {
                            if receiver.mutability.is_none() {
                                return Err(syn::Error::new(
                                    ty.span(),
                                    "mutable reflected returns require &mut self",
                                ));
                            }
                            let wrap = if matches!(reference.elem.as_ref(), Type::Slice(_)) {
                                quote!(from_slice_mut)
                            } else {
                                quote!(from_mut)
                            };
                            (
                                quote!(returning_mut),
                                quote!(::pixui_reflect::DynamicObject::#wrap(receiver.#method_name(#(#arguments),*))),
                            )
                        } else {
                            if receiver.mutability.is_some() {
                                return Err(syn::Error::new(
                                    ty.span(),
                                    "shared reflected returns currently require &self",
                                ));
                            }
                            let wrap = if matches!(reference.elem.as_ref(), Type::Slice(_)) {
                                quote!(from_slice)
                            } else {
                                quote!(from_ref)
                            };
                            (
                                quote!(returning_ref),
                                quote!(::pixui_reflect::DynamicObject::#wrap(receiver.#method_name(#(#arguments),*))),
                            )
                        }
                    }
                    _ => {
                        let constructor = if receiver.mutability.is_some() {
                            quote!(new)
                        } else {
                            quote!(shared)
                        };
                        (
                            constructor,
                            quote!(::std::boxed::Box::new(receiver.#method_name(#(#arguments),*))),
                        )
                    }
                };
                methods.push(quote!(
                    #(#block_attrs)* #(#attrs)*
                    ::pixui_reflect::Method::#constructor::<Self>(stringify!(#method_name), #arity, |receiver, arguments| {
                        #(#bindings)*
                        Ok(#result)
                    })
                ));
            }
        }
        let attrs = conditional_attributes(&structure.attrs);
        generated.push(syn::parse2::<Item>(quote!(
            #(#attrs)*
            impl ::pixui_reflect::Reflect for #name {
                fn type_descriptor() -> &'static ::pixui_reflect::TypeDescriptor {
                    static DESCRIPTOR: ::std::sync::OnceLock<::pixui_reflect::TypeDescriptor> = ::std::sync::OnceLock::new();
                    DESCRIPTOR.get_or_init(||
                        ::pixui_reflect::TypeDescriptor::new::<Self>(
                            ::std::vec![#(#fields),*], ::std::vec![#(#methods),*]
                        ).expect("automatically generated member names must be unique")
                        .with_constructor(|fields| {
                            let mut values = fields.into_iter();
                            let mut field_index = 0usize;
                            let value = #construct_value;
                            let _ = (&mut values, field_index);
                            Ok(::pixui_reflect::DynamicObject::from_reflect(value))
                        })
                    )
                }
            }
        ))?);
    }
    items.extend(generated);
    Ok(())
}

fn conditional_attributes(attributes: &[syn::Attribute]) -> Vec<syn::Attribute> {
    attributes
        .iter()
        .filter_map(|attribute| {
            conditional_meta(&attribute.meta).map(|meta| syn::parse_quote!(#[#meta]))
        })
        .collect()
}

// Preserve conditional availability without copying derives or other attributes
// onto generated impls and registration expressions.
fn conditional_meta(meta: &syn::Meta) -> Option<syn::Meta> {
    if meta.path().is_ident("cfg") {
        return Some(meta.clone());
    }
    if !meta.path().is_ident("cfg_attr") {
        return None;
    }
    let syn::Meta::List(list) = meta else {
        return None;
    };
    let entries = list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok()?;
    let mut entries = entries.into_iter();
    let condition = entries.next()?;
    let conditional: Vec<_> = entries.filter_map(|meta| conditional_meta(&meta)).collect();
    if conditional.is_empty() {
        return None;
    }
    Some(syn::parse_quote!(cfg_attr(#condition, #(#conditional),*)))
}

#[cfg(test)]
mod tests {
    use super::expand;

    #[test]
    fn rejects_unsupported_items_with_clear_diagnostics() {
        for (input, expected) in [
            ("mod model;", "inline module"),
            (
                "mod model { struct Value<T> { value: T } }",
                "cannot be generic",
            ),
            ("mod model { struct Value(i32); }", "named or unit structs"),
            (
                "mod model { struct Value; impl Value { fn consume(self) {} } }",
                "&self or &mut self",
            ),
            (
                "mod model { struct Value; impl Value { async fn run(&self) {} } }",
                "synchronous",
            ),
            (
                "mod model { struct Value; impl Value { unsafe fn run(&self) {} } }",
                "safe",
            ),
            (
                "mod model { struct Value; impl Value { fn run<T>(&self, value: T) {} } }",
                "non-generic",
            ),
            (
                "mod model { struct Value; impl Value { fn run(&self, value: &mut i32) {} } }",
                "shared references",
            ),
        ] {
            let mut module = syn::parse_str(input).unwrap();
            assert!(
                expand(&mut module)
                    .unwrap_err()
                    .to_string()
                    .contains(expected)
            );
        }
    }
}
