use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, GenericArgument, ItemFn, Pat, PathArguments, ReturnType, Type, spanned::Spanned};

pub(super) fn expand(function: ItemFn) -> syn::Result<TokenStream> {
    let signature = &function.sig;
    if signature.asyncness.is_some()
        || signature.unsafety.is_some()
        || signature.abi.is_some()
        || signature.variadic.is_some()
        || !signature.generics.params.is_empty()
        || signature.generics.where_clause.is_some()
    {
        return Err(syn::Error::new(
            signature.span(),
            "actions must be safe, synchronous, non-generic functions",
        ));
    }
    let name = &signature.ident;
    let module = format_ident!("{name}_action");
    let visibility = &function.vis;
    let application = fresh_ident(signature, "__pixui_application");
    let slice = fresh_ident(signature, "__pixui_slice");
    let request = fresh_ident(signature, "__pixui_request");
    let mut fields = Vec::new();
    let mut bindings = Vec::new();
    let mut injections = Vec::new();
    let mut arguments = Vec::new();
    let mut borrows = 0;
    for input in &signature.inputs {
        let FnArg::Typed(input) = input else {
            return Err(syn::Error::new(
                input.span(),
                "actions must be free functions",
            ));
        };
        let Pat::Ident(pattern) = input.pat.as_ref() else {
            return Err(syn::Error::new(
                input.pat.span(),
                "action parameters need simple names",
            ));
        };
        if pattern.by_ref.is_some() || pattern.subpat.is_some() {
            return Err(syn::Error::new(
                pattern.span(),
                "action parameters need simple names",
            ));
        }
        if !input.attrs.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "conditional action parameters are unsupported",
            ));
        }
        let parameter = &pattern.ident;
        let ty = &input.ty;
        if let Type::Reference(reference) = ty.as_ref() {
            if reference.mutability.is_none() || reference.lifetime.is_some() {
                return Err(syn::Error::new(
                    ty.span(),
                    "action borrows must be &mut T with an elided lifetime",
                ));
            }
            borrows += 1;
            if borrows > 1 {
                return Err(syn::Error::new(
                    ty.span(),
                    "actions currently support at most one mutable parameter",
                ));
            }
            let target = &reference.elem;
            if let Some(element) = arena_element(target)? {
                injections.push(quote!(::pixui_engine::application::action::CollectionBinding::new::<#element>(stringify!(#parameter))));
                bindings.push(quote!(let #parameter = #application.collection_mut::<#element>(#slice, stringify!(#parameter))?;));
            } else {
                fields.push(quote!(pub #parameter: ::pixui_engine::application::object_ref::ObjectRef<#target>));
                bindings.push(quote!(let #parameter = #application.resolve_mut::<#target>(#request.#parameter)?;));
            }
            arguments.push(quote!(#parameter));
        } else {
            fields.push(quote!(pub #parameter: #ty));
            arguments.push(quote!(#request.#parameter));
        }
    }

    let fallible = matches!(&signature.output, ReturnType::Type(_, ty)
        if matches!(ty.as_ref(), Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "PixuiResult")));
    if matches!(&signature.output, ReturnType::Type(_, ty) if matches!(ty.as_ref(), Type::Reference(_)))
    {
        return Err(syn::Error::new(
            signature.output.span(),
            "action outputs must be owned",
        ));
    }
    let call = if fallible {
        quote!(super::#name(#(#arguments),*)?)
    } else {
        quote!(super::#name(#(#arguments),*))
    };
    let description = function
        .attrs
        .iter()
        .filter_map(|attribute| {
            if !attribute.path().is_ident("doc") {
                return None;
            }
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(literal) = &value.value else {
                return None;
            };
            let syn::Lit::Str(text) = &literal.lit else {
                return None;
            };
            Some(text.value().trim().to_owned())
        })
        .collect::<Vec<_>>()
        .join("\n");
    let conditional = super::conditional_attributes(&function.attrs);

    Ok(quote! {
        #function

        #(#conditional)*
        #visibility mod #module {
            #[allow(unused_imports)]
            use super::*;

            #[::pixui_reflect::reflect]
            pub mod request {
                #[allow(unused_imports)]
            use super::*;
                pub struct Request { #(#fields),* }
            }

            /// Metadata and dispatch adapter generated from the action function.
            pub fn descriptor() -> &'static ::pixui_engine::application::action::ActionDescriptor {
                static DESCRIPTOR: ::std::sync::OnceLock<::pixui_engine::application::action::ActionDescriptor> = ::std::sync::OnceLock::new();
                DESCRIPTOR.get_or_init(|| {
                    ::pixui_engine::application::action::ActionDescriptor::new::<request::Request>(
                        stringify!(#name), #description, ::std::vec![#(#injections),*],
                        |#application, #slice, #request| {
                            let #request = #request.into_owned::<request::Request>()?;
                            let _ = (&#request, &#slice, &#application);
                            #(#bindings)*
                            let result = #call;
                            Ok(::std::boxed::Box::new(result))
                        },
                    )
                })
            }
        }
    })
}

// Generated locals must not shadow handler parameters with ordinary names.
fn fresh_ident(signature: &syn::Signature, prefix: &str) -> syn::Ident {
    let mut name = prefix.to_owned();
    while signature.inputs.iter().any(|input| matches!(input,
        FnArg::Typed(input) if matches!(input.pat.as_ref(), Pat::Ident(pattern) if pattern.ident == name))) {
        name.push('_');
    }
    format_ident!("{name}")
}

fn arena_element(ty: &Type) -> syn::Result<Option<&Type>> {
    let Type::Path(path) = ty else {
        return Ok(None);
    };
    let Some(segment) = path
        .path
        .segments
        .last()
        .filter(|segment| segment.ident == "Arena")
    else {
        return Ok(None);
    };
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new(
            ty.span(),
            "injected Arena requires an item type",
        ));
    };
    match arguments.args.first() {
        Some(GenericArgument::Type(element)) if arguments.args.len() == 1 => Ok(Some(element)),
        _ => Err(syn::Error::new(
            ty.span(),
            "injected Arena requires one item type",
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unsupported_signatures() {
        for (source, diagnostic) in [
            ("async fn run() {}", "synchronous"),
            ("fn run<T>(value: T) {}", "non-generic"),
            ("fn run(item: &Item) {}", "&mut T"),
            ("fn run(a: &mut Item, b: &mut Item) {}", "at most one"),
            ("fn run((a, b): (i32, i32)) {}", "simple names"),
            ("fn run() -> &Item { todo!() }", "owned"),
        ] {
            let function = syn::parse_str(source).unwrap();
            assert!(
                super::expand(function)
                    .unwrap_err()
                    .to_string()
                    .contains(diagnostic)
            );
        }
    }
}
