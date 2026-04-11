use facet::{Facet, Shape};

pub trait Reflect: 'static {
    fn shape() -> &'static Shape;
}

impl<T: Facet<'static>> Reflect for T {
    fn shape() -> &'static Shape {
        T::SHAPE
    }
}
