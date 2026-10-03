use crate::live_model::part::LivePart;
use pixui_base::PixuiResult;

pub struct Walk {}

pub struct WalkEntry<'a> {
    pub part: &'a mut LivePart,
}

pub trait Visitor {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()>;
}

pub fn walk<V: Visitor>(root: &mut LivePart, visitor: &mut V) -> PixuiResult<()> {
    let mut stack = vec![root];

    while let Some(part) = stack.pop() {
        visitor.visit(&mut WalkEntry { part })?;

        // Push children in reverse so they are visited in their original order.
        match part {
            LivePart::Composite(composite_part) => {
                stack.extend(composite_part.parts.iter_mut().rev());
            }
            LivePart::Component(_) => {}
            LivePart::ForLoop(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::{Visitor, WalkEntry, walk};
    use crate::live_model::part::{CompositePart, ForLoopPart, LivePart};
    use pixui_base::PixuiResult;

    #[derive(Default)]
    struct CollectingVisitor {
        output: String,
    }

    impl Visitor for CollectingVisitor {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            use std::fmt::Write;

            match &*entry.part {
                LivePart::Composite(part) => {
                    writeln!(self.output, "Composite({} children)", part.parts.len()).unwrap();
                }
                LivePart::Component(_) => self.output.push_str("Component\n"),
                LivePart::ForLoop(_) => self.output.push_str("ForLoop\n"),
            }
            Ok(())
        }
    }

    fn composite(parts: Vec<LivePart>) -> LivePart {
        LivePart::Composite(CompositePart { parts })
    }

    #[test]
    fn visits_tree_depth_first_in_child_order() {
        let mut root = composite(vec![
            LivePart::ForLoop(ForLoopPart {}),
            composite(vec![composite(vec![]), LivePart::ForLoop(ForLoopPart {})]),
            composite(vec![LivePart::ForLoop(ForLoopPart {})]),
        ]);
        let mut visitor = CollectingVisitor::default();

        walk(&mut root, &mut visitor).unwrap();

        expect![[r#"
            Composite(3 children)
            ForLoop
            Composite(2 children)
            Composite(0 children)
            ForLoop
            Composite(1 children)
            ForLoop
        "#]]
        .assert_eq(&visitor.output);
    }
}
