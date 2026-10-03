use crate::live_model::state::GenericComponentState;

pub enum LivePart {
    Composite(CompositePart),
    Component(ComponentPart),
    ForLoop(ForLoopPart),
}


pub struct CompositePart {
    pub parts: Vec<LivePart>
}

pub struct ComponentPart {
    state: GenericComponentState,
}


pub struct ForLoopPart {
}
