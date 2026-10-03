use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    FnArg, GenericArgument, Ident, Item, ItemMod, LitStr, Pat, PathArguments, ReturnType, Token,
    Type,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

pub(super) struct Options {
    slice: LitStr,
    facade: Ident,
}

impl Parse for Options {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut slice = None;
        let mut facade = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "slice" if slice.is_none() => slice = Some(input.parse::<LitStr>()?),
                "facade" if facade.is_none() => facade = Some(input.parse::<Ident>()?),
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown or duplicate facade option",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let slice = slice.ok_or_else(|| input.error("expected slice = \"name\""))?;
        if slice.value().is_empty() {
            return Err(syn::Error::new(
                slice.span(),
                "slice name must not be empty",
            ));
        }
        Ok(Self {
            slice,
            facade: facade.ok_or_else(|| input.error("expected facade = TypeName"))?,
        })
    }
}

pub(super) fn expand(options: Options, mut module: ItemMod) -> syn::Result<TokenStream> {
    let (_, items) = module.content.as_mut().ok_or_else(|| {
        syn::Error::new(
            module.ident.span(),
            "slice_actions requires an inline module",
        )
    })?;
    let mut fields = Vec::new();
    let mut bindings = Vec::new();
    let mut initializers = Vec::new();
    let mut descriptors = Vec::new();
    let mut methods = Vec::new();
    let mut expanded = Vec::new();
    for item in std::mem::take(items) {
        let Item::Fn(mut function) = item else {
            expanded.push(quote!(#item));
            continue;
        };
        let Some(attribute_index) = function.attrs.iter().position(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "action")
        }) else {
            expanded.push(quote!(#function));
            continue;
        };
        let attribute = function.attrs.remove(attribute_index);
        if !matches!(attribute.meta, syn::Meta::Path(_)) {
            return Err(syn::Error::new(
                attribute.span(),
                "action takes no arguments",
            ));
        }
        // Use the same signature validation and dispatch adapter as standalone actions.
        let adapter = super::action_macro::expand(function.clone())?;
        let name = &function.sig.ident;
        if ["bind", "bind_to", "bind_target", "register", "slice_id"]
            .contains(&name.to_string().as_str())
        {
            return Err(syn::Error::new(
                name.span(),
                "action name conflicts with a facade method",
            ));
        }
        let action_module = format_ident!("{name}_action");
        let field = format_ident!("__action_{name}");
        let conditional = super::conditional_attributes(&function.attrs);
        fields.push(quote!(#(#conditional)* #field: ::pixui_engine::application::action_handle::ActionHandle));
        bindings.push(quote!(#(#conditional)* let #field = slice.action_handle_checked(#action_module::descriptor())?;));
        initializers.push(quote!(#(#conditional)* #field));
        descriptors.push(quote!(#(#conditional)* #action_module::descriptor()));
        let mut parameters = Vec::new();
        let mut request_fields = Vec::new();
        for input in &function.sig.inputs {
            let FnArg::Typed(input) = input else {
                unreachable!("validated free function")
            };
            let Pat::Ident(pattern) = input.pat.as_ref() else {
                unreachable!("validated parameter")
            };
            let parameter = &pattern.ident;
            let ty = &input.ty;
            if let Type::Reference(reference) = ty.as_ref() {
                if super::action_macro::arena_element(&reference.elem)?.is_some() {
                    continue;
                }
                let target = &reference.elem;
                parameters.push(
                    quote!(#parameter: ::pixui_engine::application::object_ref::ObjectRef<#target>),
                );
                request_fields.push(quote!(#parameter));
            } else if matches!(ty.as_ref(), Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "String"))
            {
                parameters
                    .push(quote!(#parameter: impl ::std::convert::Into<::std::string::String>));
                request_fields.push(quote!(#parameter: #parameter.into()));
            } else {
                parameters.push(quote!(#parameter: #ty));
                request_fields.push(quote!(#parameter));
            }
        }
        let output = output_type(&function.sig.output)?;
        let docs: Vec<_> = function
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("doc"))
            .collect();
        methods.push(quote! {
            #(#conditional)*
            #(#docs)*
            pub fn #name(&self, #(#parameters),*) -> ::pixui_base::PixuiResult<#output> {
                let call = ::pixui_engine::application::action::ActionCall {
                    slice: self.#field.slice_id(),
                    action: self.#field.index(),
                    request: ::std::boxed::Box::new(#action_module::request::Request { #(#request_fields),* }),
                };
                let output = self.application.dispatch(call)?.wait()?;
                output.downcast::<#output>().map(|value| *value)
                    .map_err(|_| ::pixui_base::pixui_error!("action `{}` returned an unexpected output type", stringify!(#name)))
            }
        });
        expanded.push(adapter);
    }
    if fields.is_empty() {
        return Err(syn::Error::new(
            module.ident.span(),
            "slice_actions requires at least one #[action] function",
        ));
    }
    let facade = options.facade;
    let slice_name = options.slice;
    let generated = quote! {
        /// Typed synchronous access to this module's actions.
        /// Clones share the worker; each call waits for its typed result.
        /// Never call from the same application's worker.
        #[derive(Clone)]
        pub struct #facade {
            application: ::pixui_engine::application::application_handle::ApplicationHandle,
            slice: ::pixui_engine::application::application_slice::SliceId,
            #(#fields),*
        }
        impl #facade {
            /// Registers all enabled actions atomically after validating their bindings.
            pub fn register(slice: &mut ::pixui_engine::application::application_slice::ApplicationSlice) -> ::pixui_base::PixuiResult<()> {
                slice.register_actions(&[#(#descriptors),*])
            }
            /// Binds by the attribute's slice name in one worker round trip.
            /// Missing slices/actions or different handler descriptors return errors.
            pub fn bind(application: &::pixui_engine::application::application_handle::ApplicationHandle) -> ::pixui_base::PixuiResult<Self> {
                Self::bind_target(application, None)
            }
            /// Binds the same actions to another explicitly identified slice.
            pub fn bind_to(application: &::pixui_engine::application::application_handle::ApplicationHandle,
                slice: ::pixui_engine::application::application_slice::SliceId) -> ::pixui_base::PixuiResult<Self> {
                Self::bind_target(application, Some(slice))
            }
            fn bind_target(application: &::pixui_engine::application::application_handle::ApplicationHandle,
                target: Option<::pixui_engine::application::application_slice::SliceId>) -> ::pixui_base::PixuiResult<Self> {
                let application_clone = application.clone();
                application.inspect(move |state| {
                    let slice = match target {
                        Some(id) => state.slice(id)?,
                        None => state.slice_named(#slice_name)?,
                    };
                    #(#bindings)*
                    Ok(Self { application: application_clone, slice: slice.id(), #(#initializers),* })
                })
            }
            /// Stable identity of the bound slice, for constructing object references.
            pub fn slice_id(&self) -> ::pixui_engine::application::application_slice::SliceId {
                self.slice
            }
            #(#methods)*
        }
    };
    let body = quote!(#(#expanded)* #generated);
    let parsed: syn::File = syn::parse2(body)?;
    *items = parsed.items;
    Ok(quote!(#module))
}

fn output_type(output: &ReturnType) -> syn::Result<TokenStream> {
    let ReturnType::Type(_, ty) = output else {
        return Ok(quote!(()));
    };
    if let Type::Path(path) = ty.as_ref()
        && let Some(segment) = path
            .path
            .segments
            .last()
            .filter(|segment| segment.ident == "PixuiResult")
    {
        if let PathArguments::AngleBracketed(arguments) = &segment.arguments
            && arguments.args.len() == 1
            && let Some(GenericArgument::Type(ty)) = arguments.args.first()
        {
            return Ok(quote!(#ty));
        }
        return Err(syn::Error::new(
            ty.span(),
            "PixuiResult requires one output type",
        ));
    }
    Ok(quote!(#ty))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_invalid_options() {
        for source in [
            "facade = Actions",
            "slice = \"todo\"",
            "slice = \"\", facade = Actions",
            "slice = \"todo\", slice = \"other\", facade = Actions",
            "slice = \"todo\", facade = Actions, unknown = Other",
        ] {
            assert!(
                syn::parse_str::<super::Options>(source).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_modules_and_reserved_names() {
        for (source, diagnostic) in [
            ("mod actions;", "inline module"),
            ("mod actions { fn helper() {} }", "at least one"),
            ("mod actions { #[action] fn bind() {} }", "conflicts"),
            (
                "mod actions { #[action(extra)] fn run() {} }",
                "no arguments",
            ),
            ("mod actions { #[action] async fn run() {} }", "synchronous"),
        ] {
            let options = syn::parse_str("slice = \"todo\", facade = Actions").unwrap();
            let module = syn::parse_str(source).unwrap();
            assert!(
                super::expand(options, module)
                    .unwrap_err()
                    .to_string()
                    .contains(diagnostic)
            );
        }
    }
}
