//! Generates object-safe event interfaces, reflected payloads and local proxies.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{FnArg, ItemTrait, Pat, ReturnType, TraitItem, Type, spanned::Spanned};

pub(crate) fn expand(
    args: TokenStream,
    item: TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let engine: syn::Path = if args.is_empty() {
        syn::parse_quote!(::pixui_engine)
    } else {
        syn::parse2(args)?
    };
    let interface: ItemTrait = syn::parse2(item)?;
    if !interface.generics.params.is_empty()
        || !interface.supertraits.is_empty()
        || interface.unsafety.is_some()
    {
        return Err(syn::Error::new(
            interface.span(),
            "event interfaces cannot have generics, supertraits or unsafe",
        ));
    }
    let name = &interface.ident;
    let visibility = &interface.vis;
    let bindings = format_ident!("{}Bindings", name);
    let mut keys = vec![];
    let proxy = format_ident!("{}Proxy", name);
    let mut payloads = vec![];
    let mut methods = vec![];
    let mut descriptors = vec![];
    let mut seen = std::collections::HashSet::new();
    for (index, item) in interface.items.iter().enumerate() {
        let TraitItem::Fn(method) = item else {
            return Err(syn::Error::new(
                item.span(),
                "event interfaces contain only methods",
            ));
        };
        let signature = &method.sig;
        if method.default.is_some()
            || signature.asyncness.is_some()
            || signature.unsafety.is_some()
            || signature.constness.is_some()
            || signature.abi.is_some()
            || signature.variadic.is_some()
            || !signature.generics.params.is_empty()
            || signature.generics.where_clause.is_some()
        {
            return Err(syn::Error::new(
                method.span(),
                "event methods cannot have bodies, generics, async or unsafe signatures",
            ));
        }
        let Some(FnArg::Receiver(receiver)) = signature.inputs.first() else {
            return Err(syn::Error::new(
                signature.span(),
                "event methods require &self",
            ));
        };
        if receiver.reference.is_none()
            || receiver.mutability.is_some()
            || receiver.colon_token.is_some()
        {
            return Err(syn::Error::new(
                receiver.span(),
                "event methods require &self",
            ));
        }
        let ReturnType::Type(_, output) = &signature.output else {
            return Err(syn::Error::new(
                signature.span(),
                "event methods return PixuiResult<()>",
            ));
        };
        let Type::Path(output) = output.as_ref() else {
            return Err(syn::Error::new(
                output.span(),
                "event methods return PixuiResult<()>",
            ));
        };
        let valid_result = output.path.segments.last().is_some_and(|segment| {
            segment.ident == "PixuiResult" && matches!(&segment.arguments, syn::PathArguments::AngleBracketed(arguments) if arguments.args.len()==1 && matches!(arguments.args.first(),Some(syn::GenericArgument::Type(Type::Tuple(tuple))) if tuple.elems.is_empty()))
        });
        if !valid_result {
            return Err(syn::Error::new(
                output.span(),
                "event methods return PixuiResult<()>",
            ));
        }
        let method_name = &signature.ident;
        let event_name = method_name.unraw().to_string();
        let key = format_ident!("{}", event_name.to_uppercase());
        let suffix = event_name
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect::<String>();
        let payload = format_ident!("{}{}Payload", name, suffix);
        let mut parameter_types = vec![];
        let mut fields = vec![];
        let mut field_names = vec![];
        let mut metadata = vec![];
        for argument in signature.inputs.iter().skip(1) {
            let FnArg::Typed(argument) = argument else {
                unreachable!()
            };
            let Pat::Ident(pattern) = argument.pat.as_ref() else {
                return Err(syn::Error::new(
                    argument.span(),
                    "event parameters must have names",
                ));
            };
            let field = &pattern.ident;
            if pattern.by_ref.is_some()
                || pattern.subpat.is_some()
                || matches!(
                    field.to_string().as_str(),
                    "event" | "entities" | "collections" | "layouts" | "styles" | "props"
                )
            {
                return Err(syn::Error::new(
                    pattern.span(),
                    "invalid or reserved event parameter name",
                ));
            }
            if matches!(argument.ty.as_ref(), Type::Reference(_)) {
                return Err(syn::Error::new(
                    argument.ty.span(),
                    "event parameters must be owned",
                ));
            }
            let ty = &argument.ty;
            parameter_types.push(ty);
            fields.push(quote!(#field: #ty));
            field_names.push(field);
            let field_name = field.unraw().to_string();
            metadata.push(quote!(::pixui_reflect::Field::reflected::<Self, #ty>(#field_name, |payload| &payload.#field)));
        }
        if !seen.insert(method_name.to_string()) {
            return Err(syn::Error::new(method.span(), "duplicate event method"));
        }
        keys.push(quote! { pub const #key: #engine::events::binding::EventKey<#payload> = #engine::events::binding::EventKey::new(#event_name); });
        payloads.push(quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types)]
            #visibility struct #payload { #(pub #fields),* }
            impl ::pixui_reflect::Reflect for #payload {
                fn type_descriptor() -> &'static ::pixui_reflect::TypeDescriptor {
                    static DESCRIPTOR: ::std::sync::OnceLock<::pixui_reflect::TypeDescriptor> = ::std::sync::OnceLock::new();
                    DESCRIPTOR.get_or_init(|| ::pixui_reflect::TypeDescriptor::new::<Self>(vec![#(#metadata),*], vec![]).expect("generated event fields are unique"))
                }
            }
        });
        methods.push(quote! { #signature {
            self.sink.emit(#index, ::pixui_reflect::DynamicObject::from_reflect(#payload { #(#field_names),* }))
        }});
        descriptors.push(quote!(#engine::events::interface::EventDescriptor::of::<#payload>(#event_name)#(.with_parameter_type::<#parameter_types>())*));
    }
    Ok(quote! {
        #interface
        #(#payloads)*
        #visibility struct #bindings;
        impl #bindings { #(#keys)* }
        #[doc(hidden)]
        #visibility struct #proxy { sink: #engine::events::interface::EventSink }
        impl #name for #proxy { #(#methods)* }
        impl #engine::events::interface::EventInterface for dyn #name {
            fn descriptors() -> &'static [#engine::events::interface::EventDescriptor] {
                static EVENTS: ::std::sync::OnceLock<Vec<#engine::events::interface::EventDescriptor>> = ::std::sync::OnceLock::new();
                EVENTS.get_or_init(|| vec![#(#descriptors),*])
            }
            fn with_proxy<R>(sink: &#engine::events::interface::EventSink, use_proxy: impl FnOnce(&Self) -> R) -> R {
                use_proxy(&#proxy { sink: sink.clone() })
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    #[test]
    fn rejects_unsupported_interfaces_with_targeted_diagnostics() {
        for (source, expected) in [
            ("trait E { fn x(&mut self) -> PixuiResult<()>; }", "&self"),
            ("trait E { fn x() -> PixuiResult<()>; }", "&self"),
            ("trait E { async fn x(&self) -> PixuiResult<()>; }", "async"),
            ("trait E { fn x<T>(&self) -> PixuiResult<()>; }", "generics"),
            (
                "trait E { fn x(&self, text: &str) -> PixuiResult<()>; }",
                "owned",
            ),
            (
                "trait E { fn x(&self, event: String) -> PixuiResult<()>; }",
                "reserved",
            ),
            (
                "trait E { fn x(&self, props: String) -> PixuiResult<()>; }",
                "reserved",
            ),
            ("trait E { fn x(&self) -> bool; }", "PixuiResult"),
            (
                "trait E { fn x(&self) -> PixuiResult<()> { Ok(()) } }",
                "bodies",
            ),
            ("trait E { type X; }", "only methods"),
            ("trait E<T> {}", "generics"),
            ("trait E: Send {}", "supertraits"),
        ] {
            let error = expand(Default::default(), source.parse().unwrap()).unwrap_err();
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
    }
    #[test]
    fn supports_empty_interfaces_and_generates_named_metadata() {
        assert!(expand(Default::default(), "pub trait Empty {}".parse().unwrap()).is_ok());
        let generated = expand(
            Default::default(),
            "pub trait E { fn change(&self, content: String) -> PixuiResult<()>; }"
                .parse()
                .unwrap(),
        )
        .unwrap()
        .to_string();
        assert!(generated.contains("EBindings"));
        assert!(generated.contains("CHANGE"));
        assert!(generated.contains("EventDescriptor"));
    }
}
