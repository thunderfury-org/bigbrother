use std::ops::Range;

use super::labels::Label;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub span: Range<usize>,
    pub text: String,
    pub kind: TokenKind,
    pub label: Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Bare,
    Bracketed,
    Parenthesized,
}

const SEPARATORS: &[char] = &['.', '_', ' ', '/'];

pub(crate) fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

        // Bracketed content: [...]
        if ch == '[' {
            push_bare(&mut tokens, input, pos, byte_pos);
            let inner_start = byte_pos + ch.len_utf8();
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < chars.len() {
                match chars[j].1 {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j < chars.len() {
                let inner_end = chars[j].0;
                let end = chars[j].0 + chars[j].1.len_utf8();
                tokens.push(Token {
                    span: inner_start..inner_end,
                    text: input[inner_start..inner_end].to_owned(),
                    kind: TokenKind::Bracketed,
                    label: Label::default(),
                });
                pos = end;
                i = j + 1;
            } else {
                i += 1;
            }
            continue;
        }

        // Parenthesized content: (...)
        if ch == '(' {
            push_bare(&mut tokens, input, pos, byte_pos);
            let inner_start = byte_pos + ch.len_utf8();
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < chars.len() {
                match chars[j].1 {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j < chars.len() {
                let inner_end = chars[j].0;
                let end = chars[j].0 + chars[j].1.len_utf8();
                tokens.push(Token {
                    span: inner_start..inner_end,
                    text: input[inner_start..inner_end].to_owned(),
                    kind: TokenKind::Parenthesized,
                    label: Label::default(),
                });
                pos = end;
                i = j + 1;
            } else {
                i += 1;
            }
            continue;
        }

        // Separator: skip
        if SEPARATORS.contains(&ch) {
            push_bare(&mut tokens, input, pos, byte_pos);
            pos = byte_pos + ch.len_utf8();
            i += 1;
            continue;
        }

        i += 1;
    }

    // trailing bare token
    push_bare(&mut tokens, input, pos, input.len());

    tokens
}

fn push_bare(tokens: &mut Vec<Token>, input: &str, start: usize, end: usize) {
    if start >= end {
        return;
    }

    let text = input[start..end].to_owned();
    if text.is_empty() {
        return;
    }

    tokens.push(Token {
        span: start..end,
        text,
        kind: TokenKind::Bare,
        label: Label::default(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_bare_word_yields_one_bare_token() {
        let tokens = tokenize("Title");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "Title");
        assert_eq!(tokens[0].kind, TokenKind::Bare);
        // round-trip: span must point at the actual text
        assert_eq!(&"Title"[tokens[0].span.clone()], "Title");
    }

    #[test]
    fn underscore_and_space_separators() {
        let input = "Some_Title Here";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Bare));
        assert_eq!(tokens[0].text, "Some");
        assert_eq!(tokens[1].text, "Title");
        assert_eq!(tokens[2].text, "Here");
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }

    #[test]
    fn dot_separated_bare_tokens() {
        let input = "Title.2024.1080p";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Bare));
        assert_eq!(tokens[0].text, "Title");
        assert_eq!(tokens[1].text, "2024");
        assert_eq!(tokens[2].text, "1080p");
        // round-trip
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }

    #[test]
    fn slash_separated_bare_tokens() {
        let input = "Title/Alternate";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, "Title");
        assert_eq!(tokens[1].text, "Alternate");
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Bare));
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }

    #[test]
    fn parenthesized_content_yields_parenthesized_token() {
        let input = "Title (2024)";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Bare);
        assert_eq!(tokens[0].text, "Title");
        assert_eq!(tokens[1].kind, TokenKind::Parenthesized);
        assert_eq!(tokens[1].text, "2024");
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }

    #[test]
    fn bare_text_before_bracket_is_preserved() {
        let input = "Title[Group]";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Bare);
        assert_eq!(tokens[0].text, "Title");
        assert_eq!(tokens[1].kind, TokenKind::Bracketed);
        assert_eq!(tokens[1].text, "Group");
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }

    #[test]
    fn bracketed_prefix_yields_bracketed_and_bare_token() {
        let input = "[Group] Title";
        let tokens = tokenize(input);

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Bracketed);
        assert_eq!(tokens[0].text, "Group");
        assert_eq!(tokens[1].kind, TokenKind::Bare);
        assert_eq!(tokens[1].text, "Title");
        for t in &tokens {
            assert_eq!(&input[t.span.clone()], t.text.as_str());
        }
    }
}
