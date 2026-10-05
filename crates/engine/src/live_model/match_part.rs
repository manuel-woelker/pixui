//! Ordered value matching for conditional live subtrees.

use super::part::LivePart;
use crate::expression::expression::Expression;
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, Reflect};
use std::{
    any::{TypeId, type_name},
    sync::Arc,
};

type Comparison = dyn for<'a> Fn(&DynamicObject<'a>) -> PixuiResult<bool> + Send + Sync;

/// Opaque immutable comparison. Construct through MatchPattern::value so type
/// metadata and the erased comparison cannot disagree.
#[derive(Clone)]
pub struct ValuePattern {
    type_id: TypeId,
    type_name: &'static str,
    compare: Arc<Comparison>,
}

/// Exact typed equality or a final catch-all. This is not Rust destructuring:
/// guards, bindings, coercions, ranges, and exhaustiveness checks are absent.
#[derive(Clone)]
pub enum MatchPattern {
    Value(ValuePattern),
    Wildcard,
}
impl MatchPattern {
    /// Uses T's PartialEq without requiring Clone or adding reflection equality.
    /// NaN follows PartialEq and never equals another NaN.
    pub fn value<T: Reflect + PartialEq + Send + Sync>(expected: T) -> Self {
        Self::Value(ValuePattern {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            compare: Arc::new(move |value| {
                let actual = value.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("match value must expose `{}`", type_name::<T>())
                })?;
                Ok(*actual == expected)
            }),
        })
    }
}

/// One arm in declaration order. Only the selected part is walked.
#[derive(Clone)]
pub struct MatchCandidate {
    pub pattern: MatchPattern,
    pub part: LivePart,
}

/// Evaluate once after the node visitor; walk the first matching arm without
/// replacing the enclosing context. No match renders nothing. Only the active
/// arm retains physical state, by candidate position. Switching unmounts it.
#[derive(Clone)]
pub struct MatchPart {
    pub expression: Expression,
    pub candidates: Vec<MatchCandidate>,
}
impl MatchPart {
    /// Rejects mixed concrete value types and arms after a wildcard. Duplicates
    /// are allowed: first wins. Public template edits are revalidated on walking.
    pub fn new(expression: Expression, candidates: Vec<MatchCandidate>) -> PixuiResult<Self> {
        let part = Self {
            expression,
            candidates,
        };
        part.validate()?;
        Ok(part)
    }
    pub(crate) fn validate(&self) -> PixuiResult<()> {
        let mut value_type = None;
        for (index, candidate) in self.candidates.iter().enumerate() {
            match &candidate.pattern {
                MatchPattern::Value(pattern) => {
                    if value_type.is_some_and(|existing| existing != pattern.type_id) {
                        return Err(pixui_error!(
                            "match candidates must use the same concrete value type"
                        ));
                    }
                    value_type = Some(pattern.type_id);
                }
                MatchPattern::Wildcard if index + 1 != self.candidates.len() => {
                    return Err(pixui_error!("match wildcard must be the last candidate"));
                }
                MatchPattern::Wildcard => {}
            }
        }
        Ok(())
    }
    pub(crate) fn select(&self, value: &DynamicObject<'_>) -> PixuiResult<Option<usize>> {
        self.validate()?;
        // Check before testing any arm, so a wildcard cannot mask a type error.
        if let Some(pattern) =
            self.candidates
                .iter()
                .find_map(|candidate| match &candidate.pattern {
                    MatchPattern::Value(pattern) => Some(pattern),
                    _ => None,
                })
            && value.descriptor().type_id() != pattern.type_id
        {
            return Err(pixui_error!(
                "match expected `{}`, got `{}`",
                pattern.type_name,
                value.descriptor().type_name()
            ));
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            if match &candidate.pattern {
                MatchPattern::Value(pattern) => (pattern.compare)(value)?,
                MatchPattern::Wildcard => true,
            } {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_model::part::CompositePart;

    fn part(patterns: Vec<MatchPattern>) -> MatchPart {
        MatchPart::new(
            Expression::field(pixui_reflect::FieldIndex(0)),
            patterns
                .into_iter()
                .map(|pattern| MatchCandidate {
                    pattern,
                    part: LivePart::Composite(CompositePart { parts: vec![] }),
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn comparable_values_use_exact_types_and_partial_equality() {
        assert_eq!(
            part(vec![
                MatchPattern::value(String::from("yes")),
                MatchPattern::Wildcard
            ])
            .select(&DynamicObject::from_reflect(String::from("no")))
            .unwrap(),
            Some(1)
        );
        assert_eq!(
            part(vec![MatchPattern::value(4i32), MatchPattern::value(4i32)])
                .select(&DynamicObject::from_reflect(4i32))
                .unwrap(),
            Some(0)
        );
        assert!(
            part(vec![MatchPattern::value(4i32), MatchPattern::Wildcard])
                .select(&DynamicObject::from_reflect(4i64))
                .is_err()
        );
        assert_eq!(
            part(vec![MatchPattern::value(f64::NAN)])
                .select(&DynamicObject::from_reflect(f64::NAN))
                .unwrap(),
            None
        );
    }
}
