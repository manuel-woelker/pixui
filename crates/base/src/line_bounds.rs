/// Line metadata for a byte index within a source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineBounds {
    pub line_number: usize,
    pub line_start: usize,
    pub line_end: usize,
}

impl LineBounds {
    /// Creates line metadata for the line containing `index`.
    pub fn new(source: &str, index: usize) -> Self {
        let line_start = source[..index].rfind('\n').map_or(0, |offset| offset + 1);
        let line_end = source[index..]
            .find('\n')
            .map_or(source.len(), |offset| index + offset);
        let line_number = source[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;

        Self {
            line_number,
            line_start,
            line_end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LineBounds;

    #[test]
    fn reports_bounds_for_the_first_line() {
        let line_bounds = LineBounds::new("alpha\nbeta", 2);

        assert_eq!(line_bounds.line_number, 1);
        assert_eq!(line_bounds.line_start, 0);
        assert_eq!(line_bounds.line_end, 5);
    }

    #[test]
    fn reports_bounds_for_a_later_line() {
        let line_bounds = LineBounds::new("alpha\nbeta\ngamma", 8);

        assert_eq!(line_bounds.line_number, 2);
        assert_eq!(line_bounds.line_start, 6);
        assert_eq!(line_bounds.line_end, 10);
    }

    #[test]
    fn reports_bounds_for_the_last_line_without_a_trailing_newline() {
        let line_bounds = LineBounds::new("alpha\nbeta", 9);

        assert_eq!(line_bounds.line_number, 2);
        assert_eq!(line_bounds.line_start, 6);
        assert_eq!(line_bounds.line_end, 10);
    }
}
