use crate::reflection::Reflect;

pub mod registry;

pub trait Component {
    type Properties: Reflect;
}
