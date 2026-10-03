pub enum LivePart {
    Composite(CompositePart),
    Component(ComponentPart),
    ForLoop(ForLoopPart),
}

pub struct CompositePart {
    pub parts: Vec<LivePart>,
}

pub struct ComponentPart {
    //    state: GenericComponentState,
}

pub struct ForLoopPart {
    /// Index of a reflected sequence field in the current context descriptor.
    pub field_index: usize,
    /// Reused for every element, with that element as the walking context.
    pub body: Box<LivePart>,
}
