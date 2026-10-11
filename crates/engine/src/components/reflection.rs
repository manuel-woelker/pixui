//! Standard scalar props opt into language construction; state stays unreflected.
use pixui_reflect::{Field, Reflect, TypeDescriptor};
use std::sync::OnceLock;

macro_rules! props {
    ($ty:ty { $($field:ident: $value:ty),* $(,)? }) => {
        impl Reflect for $ty {
            fn type_descriptor() -> &'static TypeDescriptor {
                static DESCRIPTOR: OnceLock<TypeDescriptor> = OnceLock::new();
                DESCRIPTOR.get_or_init(|| TypeDescriptor::new::<Self>(
                    vec![$(Field::reflected::<Self,$value>(stringify!($field),|props|&props.$field)),*],vec![]
                ).expect("standard props have unique fields").with_send_constructor(|values| {
                    let mut values=values.into_iter();
                    let mut index=0;
                    let props=Self {$($field:{
                        let value=values.next().expect("descriptor validates field count");
                        let field=pixui_reflect::construction::take_send::<$value>(value,index,stringify!($field))?;
                        index+=1;
                        field
                    }),*};
                    let _=index;
                    Ok(Box::new(props))
                }))
            }
        }
    };
}
props!(super::label::LabelProps { text: String });
props!(super::button::ButtonProps { label: String });
props!(super::checkbox::CheckboxProps {
    label: String,
    checked: bool
});
