mod component_events;

// Rust requires proc-macro entry points at the crate root.
#[proc_macro_attribute]
pub fn component_events(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    component_events::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
