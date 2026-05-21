use std::ops::Range;

#[derive(Debug, Clone)]
pub(crate) struct SpanMask {
    occupied: Vec<bool>,
}

impl SpanMask {
    pub(crate) fn new(body: &str) -> Self {
        Self {
            occupied: vec![false; body.len()],
        }
    }

    pub(crate) fn mark(&mut self, span: Range<usize>) {
        let end = span.end.min(self.occupied.len());
        for index in span.start.min(end)..end {
            self.occupied[index] = true;
        }
    }

    pub(crate) fn unoccupied_text(&self, body: &str) -> String {
        let mut text = String::with_capacity(body.len());
        for (index, ch) in body.char_indices() {
            let end = index + ch.len_utf8();
            if self.occupied[index..end].iter().any(|used| *used) {
                text.push(' ');
            } else {
                text.push(ch);
            }
        }
        text
    }

    pub(crate) fn first_occupied_index(&self) -> Option<usize> {
        self.occupied.iter().position(|used| *used)
    }

    pub(crate) fn has_recent_occupied_before(&self, index: usize, window: usize) -> bool {
        let window_start = index.saturating_sub(window);
        self.occupied
            .get(window_start..index)
            .is_some_and(|slice| slice.iter().any(|used| *used))
    }

    pub(crate) fn as_slice(&self) -> &[bool] {
        &self.occupied
    }
}

#[cfg(test)]
mod tests {
    use super::SpanMask;

    #[test]
    fn replaces_marked_multibyte_segments_with_spaces() {
        let body = "标题.S01E02.1080p";
        let mut mask = SpanMask::new(body);
        mask.mark("标题.".len().."标题.S01E02".len());

        assert_eq!(mask.unoccupied_text(body), "标题.      .1080p");
    }
}
