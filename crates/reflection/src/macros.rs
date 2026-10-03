/// Builds a descriptor with typed field getters and method adapters.
///
/// Returns `PixuiResult<TypeDescriptor>`. Field names come from identifiers;
/// method names, arities, and adapters are explicit. See the crate example.
#[macro_export]
macro_rules! type_descriptor {
    ($ty:ty, fields: [$($field:ident),* $(,)?], methods: [$($name:literal => ($arity:expr, $invoke:expr)),* $(,)?] $(,)?) => {
        $crate::TypeDescriptor::new::<$ty>(
            ::std::vec![$($crate::Field::new::<$ty>(::std::stringify!($field), |receiver| &receiver.$field)),*],
            ::std::vec![$($crate::Method::new::<$ty>($name, $arity, $invoke)),*],
        )
    };
}
