use super::{CssScanCursor, StringBodyScanStop};
use crate::lexer::{CssLexContext, CssStringQuote};
use biome_css_syntax::{CssNumberScanOptions, scan_css_number};
use smallvec::SmallVec;
use std::collections::HashSet;

impl CssScanCursor<'_> {
    /// Returns the absolute position of the closing `)` for an `at-rule()`
    /// body, starting immediately after its opening `(`.
    ///
    /// Unclosed strings or delimiters, mismatched closers, and a top-level
    /// semicolon return `None` without changing the lexer's state. Failed
    /// scans cache only parenthesis bodies that remain open at the failure.
    pub(crate) fn scan_supports_at_rule_fallback_end(
        self,
        failed_starts: &mut HashSet<usize>,
    ) -> Option<usize> {
        let mut scanner = SupportsBodyScanner {
            cursor: self,
            closing: SmallVec::new(),
        };
        let end = scanner.scan();
        if end.is_none() {
            failed_starts.extend(scanner.closing.iter().filter_map(|frame| match frame {
                SupportsBodyFrame::Parenthesis { body_start } => Some(*body_start),
                _ => None,
            }));
        }
        end
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SupportsBodyFrame {
    Parenthesis { body_start: usize },
    Delimiter(u8),
    String(CssStringQuote),
}

struct SupportsBodyScanner<'src> {
    cursor: CssScanCursor<'src>,
    closing: SmallVec<[SupportsBodyFrame; 4]>,
}

impl SupportsBodyScanner<'_> {
    fn scan(&mut self) -> Option<usize> {
        self.closing.push(SupportsBodyFrame::Parenthesis {
            body_start: self.cursor.position(),
        });

        while let Some(current) = self.cursor.current_byte() {
            if let Some(SupportsBodyFrame::String(quote)) = self.closing.last().copied() {
                self.scan_string(quote)?;
                continue;
            }

            match current {
                b'/' if self.cursor.peek_byte() == Some(b'*') => {
                    self.cursor.consume_block_comment();
                }
                b'/' if self.cursor.is_at_line_comment() => {
                    self.cursor.consume_line_comment();
                }
                b'\'' | b'"' => {
                    let quote = CssStringQuote::try_from(current).ok()?;
                    self.cursor.advance(1);
                    self.closing.push(SupportsBodyFrame::String(quote));
                }
                b'(' => {
                    self.cursor.advance(1);
                    self.closing.push(SupportsBodyFrame::Parenthesis {
                        body_start: self.cursor.position(),
                    });
                }
                b'[' | b'{' => {
                    let closing = match current {
                        b'[' => b']',
                        _ => b'}',
                    };
                    self.cursor.advance(1);
                    self.closing.push(SupportsBodyFrame::Delimiter(closing));
                }
                b')' | b']' | b'}' => {
                    let matches = match self.closing.last() {
                        Some(SupportsBodyFrame::Parenthesis { .. }) => current == b')',
                        Some(SupportsBodyFrame::Delimiter(closing)) => current == *closing,
                        _ => false,
                    };
                    if !matches {
                        return None;
                    }
                    self.closing.pop();
                    if self.closing.is_empty() {
                        return Some(self.cursor.position());
                    }
                    self.cursor.advance(1);
                }
                b';' if self.closing.len() == 1 => return None,
                b'\\' if !self.cursor.is_valid_escape_at(1) => return None,
                b'#' if self.consume_color() => {}
                b'#' => {
                    self.cursor.advance(1);
                    self.cursor.advance_ident_sequence();
                }
                b'@' => {
                    self.cursor.advance(1);
                    // The name in `@url(...)` belongs to the at-keyword.
                    if self.cursor.is_ident_start() {
                        self.cursor.advance_ident_sequence();
                    }
                }
                b'u' | b'U' if self.cursor.peek_byte() == Some(b'+') => {
                    self.consume_unicode_range();
                }
                _ if self.consume_number() => {}
                _ if self.cursor.is_ident_start() => self.consume_identifier()?,
                _ => self.cursor.advance_byte_or_char(current),
            }
        }

        None
    }

    fn scan_string(&mut self, quote: CssStringQuote) -> Option<()> {
        let scan = if self.cursor.is_scss {
            self.cursor.scan_interpolated_string_body(quote)
        } else {
            self.cursor.scan_plain_string_body(quote)
        };

        match scan.stop {
            StringBodyScanStop::ClosingQuote { position } => {
                self.cursor.cursor.set_position(position + 1);
                self.closing.pop();
            }
            StringBodyScanStop::Interpolation { position } => {
                self.cursor.cursor.set_position(position + 2);
                self.closing.push(SupportsBodyFrame::Delimiter(b'}'));
            }
            StringBodyScanStop::Newline { .. } | StringBodyScanStop::Eof { .. } => return None,
        }

        Some(())
    }

    fn consume_color(&mut self) -> bool {
        let mut color = self.cursor;
        color.advance(1);
        color.skip_scss_expression_trivia();
        let start = color.position();
        while color
            .current_byte()
            .is_some_and(|byte| byte.is_ascii_hexdigit())
        {
            color.advance(1);
        }

        if !matches!(color.position() - start, 3 | 4 | 6 | 8) {
            return false;
        }

        // Color context stops before `url` in `#fffurl(...)`.
        self.cursor = color;
        true
    }

    fn consume_unicode_range(&mut self) {
        self.cursor.advance(2);
        self.cursor.skip_scss_expression_trivia();

        // Unicode-range context leaves the following URL as its own function,
        // including after the second codepoint in `U+1-2url(...)`.
        let start = self.cursor.position();
        self.consume_unicode_codepoint();
        if self.cursor.position() == start {
            return;
        }

        self.cursor.skip_scss_expression_trivia();
        if self.cursor.current_byte() == Some(b'-') {
            self.cursor.advance(1);
            self.cursor.skip_scss_expression_trivia();
            self.consume_unicode_codepoint();
        }
    }

    fn consume_unicode_codepoint(&mut self) {
        while self
            .cursor
            .current_byte()
            .is_some_and(|byte| byte.is_ascii_hexdigit() || byte == b'?')
        {
            self.cursor.advance(1);
        }
    }

    fn consume_number(&mut self) -> bool {
        let Some(end) = scan_css_number(
            self.cursor.cursor.source(),
            self.cursor.position(),
            CssNumberScanOptions::default().with_scss_ellipsis_boundary(self.cursor.is_scss),
        ) else {
            return false;
        };

        self.cursor.cursor.set_position(end);
        // The unit in `1url(...)` belongs to a dimension, not a URL function.
        if self.cursor.is_ident_start() {
            self.cursor.advance_ident_sequence();
        }
        true
    }

    fn consume_identifier(&mut self) -> Option<()> {
        let mut name = [0; 3];
        let scan = self.cursor.consume_ident_sequence(&mut name);
        let is_url = scan.count == name.len()
            && scan.only_ascii_used
            && scan.last_was_buffered
            && matches!(&name, b"url" | b"src");
        if !is_url {
            return Some(());
        }

        let mut body = self.cursor;
        body.skip_scss_expression_trivia();
        if body.current_byte() != Some(b'(') {
            return Some(());
        }
        body.advance(1);
        self.cursor = body;
        self.closing.push(SupportsBodyFrame::Parenthesis {
            body_start: body.position(),
        });

        // CSS-only URL parsing keeps a function-shaped first argument on
        // regular tokens, as in `src(var(--asset))`.
        let mut first = body;
        first.skip_scss_expression_trivia();
        if first.is_ident_start() {
            first.advance_ident_sequence();
            first.skip_scss_expression_trivia();
            if first.current_byte() == Some(b'(') {
                return Some(());
            }
        }

        if let CssLexContext::UrlRawValue(scan) = body.url_body_lex_context(false) {
            if !scan.terminated {
                return None;
            }
            self.cursor.cursor.set_position(scan.end);
        }

        Some(())
    }
}
