//! Pure single-line editing. Committed text always comes from component props.
use pixui_base::{PixuiResult, pixui_error};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}
impl Selection {
    pub fn range(self) -> std::ops::Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
    pub fn collapsed(self) -> bool {
        self.anchor == self.head
    }
    pub fn clamp(self, content: &str) -> Self {
        Self {
            anchor: boundary(content, self.anchor),
            head: boundary(content, self.head),
        }
    }
}
fn boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain([content.len()])
        .take_while(|index| *index <= offset)
        .last()
        .unwrap_or(0)
}

pub fn validate(content: &str) -> PixuiResult<()> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(pixui_error!("text input exceeds the 1 MiB content limit"));
    }
    if content.chars().any(char::is_control) {
        return Err(pixui_error!(
            "text input content must be single-line and contain no control characters"
        ));
    }
    Ok(())
}

/// Clipboard/IME text is flattened before proposing it; props are never rewritten.
pub fn normalize_insert(content: &str) -> PixuiResult<String> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(pixui_error!("text insertion exceeds the 1 MiB limit"));
    }
    let mut result = String::new();
    let mut separator = false;
    for ch in content.chars() {
        if matches!(ch, '\r' | '\n' | '\t') {
            if !separator {
                result.push(' ');
            }
            separator = true;
        } else if !ch.is_control() {
            result.push(ch);
            separator = false;
        }
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Left,
    Right,
    Start,
    End,
}
pub enum Edit {
    Insert(String),
    Delete {
        backwards: bool,
        word: bool,
    },
    Move {
        direction: Direction,
        extend: bool,
        word: bool,
    },
    SelectAll,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub content: String,
    pub selection: Selection,
}

/// Shared by all instances of an occurrence. Content is a cached authoritative
/// snapshot for reconciliation, never an optimistic editable buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditState {
    pub content: String,
    pub selection: Selection,
    pub content_revision: u64,
    pub selection_revision: u64,
    pub blink_reset_us: u64,
    pub composing: bool,
    pub(crate) pending: Option<Proposal>,
}
impl EditState {
    pub fn reconcile(&mut self, content: &str) -> PixuiResult<()> {
        validate(content)?;
        let changed = self.content != content;
        if let Some(proposal) = self.pending.take()
            && (proposal.content == content || changed)
        {
            self.selection = proposal.selection.clamp(content);
        }
        self.selection = self.selection.clamp(content);
        if changed {
            self.content = content.to_owned();
            self.content_revision = self
                .content_revision
                .checked_add(1)
                .ok_or_else(|| pixui_error!("text content revision exhausted"))?;
            self.composing = false;
        }
        Ok(())
    }
    pub fn select(&mut self, selection: Selection, timestamp_us: u64) -> PixuiResult<()> {
        let selection = selection.clamp(&self.content);
        if self.selection != selection {
            self.selection_revision = self
                .selection_revision
                .checked_add(1)
                .ok_or_else(|| pixui_error!("text selection revision exhausted"))?;
            self.selection = selection;
        }
        self.blink_reset_us = timestamp_us;
        Ok(())
    }
    pub fn edit(&mut self, edit: Edit, timestamp_us: u64) -> PixuiResult<Option<Proposal>> {
        self.selection = self.selection.clamp(&self.content);
        let range = self.selection.range();
        match edit {
            Edit::Move {
                direction,
                extend,
                word,
            } => {
                let head = if !extend
                    && !self.selection.collapsed()
                    && matches!(direction, Direction::Left | Direction::Right)
                {
                    if matches!(direction, Direction::Left) {
                        range.start
                    } else {
                        range.end
                    }
                } else {
                    destination(&self.content, self.selection.head, direction, word)
                };
                self.select(
                    Selection {
                        anchor: if extend { self.selection.anchor } else { head },
                        head,
                    },
                    timestamp_us,
                )?;
                Ok(None)
            }
            Edit::SelectAll => {
                self.select(
                    Selection {
                        anchor: 0,
                        head: self.content.len(),
                    },
                    timestamp_us,
                )?;
                Ok(None)
            }
            Edit::Insert(text) => self.replace(range, &normalize_insert(&text)?, timestamp_us),
            Edit::Delete { backwards, word } => {
                let range = if self.selection.collapsed() {
                    let other = destination(
                        &self.content,
                        self.selection.head,
                        if backwards {
                            Direction::Left
                        } else {
                            Direction::Right
                        },
                        word,
                    );
                    self.selection.head.min(other)..self.selection.head.max(other)
                } else {
                    range
                };
                self.replace(range, "", timestamp_us)
            }
        }
    }
    fn replace(
        &mut self,
        range: std::ops::Range<usize>,
        inserted: &str,
        timestamp_us: u64,
    ) -> PixuiResult<Option<Proposal>> {
        if self.content.len() - range.len() + inserted.len() > MAX_CONTENT_BYTES {
            return Err(pixui_error!("text input exceeds the 1 MiB content limit"));
        }
        let mut content = self.content.clone();
        content.replace_range(range.clone(), inserted);
        if content == self.content {
            return Ok(None);
        }
        let desired = range.start + inserted.len();
        let head = content
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([content.len()])
            .find(|index| *index >= desired)
            .unwrap_or(content.len());
        self.blink_reset_us = timestamp_us;
        Ok(Some(Proposal {
            content,
            selection: Selection { anchor: head, head },
        }))
    }
}
fn destination(content: &str, head: usize, direction: Direction, word: bool) -> usize {
    match direction {
        Direction::Start => 0,
        Direction::End => content.len(),
        Direction::Left if word => content
            .unicode_word_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index < head)
            .last()
            .unwrap_or(0),
        Direction::Right if word => content
            .unicode_word_indices()
            .map(|(index, word)| index + word.len())
            .find(|index| *index > head)
            .unwrap_or(content.len()),
        Direction::Left => content
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|index| *index < head)
            .last()
            .unwrap_or(0),
        Direction::Right => content
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .find(|index| *index > head)
            .unwrap_or(content.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(content: &str) -> EditState {
        let mut state = EditState::default();
        state.reconcile(content).unwrap();
        state
    }
    #[test]
    fn edits_are_proposals_and_graphemes_cannot_be_split() {
        let content = "á👩‍👩‍👧‍👦日本";
        let mut state = state(content);
        state
            .edit(
                Edit::Move {
                    direction: Direction::Right,
                    extend: false,
                    word: false,
                },
                0,
            )
            .unwrap();
        assert_eq!(state.selection.head, "á".len());
        state
            .edit(
                Edit::Move {
                    direction: Direction::Right,
                    extend: true,
                    word: false,
                },
                0,
            )
            .unwrap();
        let proposal = state.edit(Edit::Insert("X".into()), 0).unwrap().unwrap();
        assert_eq!(proposal.content, "áX日本");
        assert_eq!(state.content, content, "no optimistic text");
        state.pending = Some(proposal);
        state.reconcile("áX日本").unwrap();
        assert_eq!(state.selection.head, "áX".len());
    }
    #[test]
    fn rejection_normalization_and_external_values_reconcile_selection() {
        let mut state = state("hello");
        state.select(Selection { anchor: 5, head: 5 }, 0).unwrap();
        state.pending = state.edit(Edit::Insert("!".into()), 0).unwrap();
        state.reconcile("hello").unwrap();
        assert_eq!(state.selection.head, 5);
        state.pending = state.edit(Edit::Insert("!".into()), 0).unwrap();
        state.reconcile("HI").unwrap();
        assert_eq!(state.selection.head, 2);
        state.reconcile("é").unwrap();
        assert_eq!(state.selection.head, 2);
        state.reconcile("").unwrap();
        assert_eq!(state.selection, Selection::default());
    }
    #[test]
    fn direction_words_deletion_and_noops() {
        let mut state = state("one 日本 three");
        state.edit(Edit::SelectAll, 0).unwrap();
        state
            .edit(
                Edit::Move {
                    direction: Direction::Left,
                    extend: false,
                    word: false,
                },
                0,
            )
            .unwrap();
        assert_eq!(state.selection.head, 0);
        assert!(
            state
                .edit(
                    Edit::Delete {
                        backwards: true,
                        word: false
                    },
                    0
                )
                .unwrap()
                .is_none()
        );
        state
            .edit(
                Edit::Move {
                    direction: Direction::End,
                    extend: false,
                    word: false,
                },
                0,
            )
            .unwrap();
        assert_eq!(
            state
                .edit(
                    Edit::Delete {
                        backwards: true,
                        word: true
                    },
                    0
                )
                .unwrap()
                .unwrap()
                .content,
            "one 日本 "
        );
        state
            .select(
                Selection {
                    anchor: state.content.len(),
                    head: 0,
                },
                0,
            )
            .unwrap();
        assert_eq!(
            state
                .edit(Edit::Insert("a".into()), 0)
                .unwrap()
                .unwrap()
                .content,
            "a"
        );
    }
    #[test]
    fn single_line_limits_and_control_normalization() {
        assert_eq!(normalize_insert("a\r\n\t\tb\0c").unwrap(), "a bc");
        assert!(validate("a\nb").is_err());
        assert!(validate(&"x".repeat(MAX_CONTENT_BYTES + 1)).is_err());
        assert!(normalize_insert(&"x".repeat(MAX_CONTENT_BYTES + 1)).is_err());
        let mut state = state("");
        assert!(state.edit(Edit::Insert("".into()), 0).unwrap().is_none());
    }
}
