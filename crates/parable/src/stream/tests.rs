use super::TokenStream;
use crate::types::Token;

use core::assert_matches;
use std::borrow::Cow;
use winnow::stream::{Offset as _, Stream as _};

#[derive(Debug, thiserror::Error)]
enum TestError {}

mod peek_token {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn repeated_peek_returns_same_token_without_consuming() -> Result<(), TestError> {
        let stream = TokenStream::new("foo bar");

        assert_eq!(stream.peek_token(), stream.peek_token());

        Ok(())
    }

    #[rstest]
    fn peek_then_next_token_consumes_the_buffered_token() -> Result<(), TestError> {
        let mut stream = TokenStream::new("foo bar");

        let peeked = stream.peek_token();
        let consumed = stream.next_token();

        assert_eq!(peeked, consumed);
        assert_eq!(stream.next_token(), Some(Token::Word(Cow::Borrowed("bar"))));

        Ok(())
    }

    #[rstest]
    fn peek_does_not_swallow_a_lex_failure() -> Result<(), TestError> {
        let mut stream = TokenStream::new("\r");

        assert_eq!(stream.peek_token(), None);
        assert_matches!(stream.take_lex_error(), Some(_));

        Ok(())
    }
}

mod offset_from {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn peek_alone_reports_zero_progress() -> Result<(), TestError> {
        let stream = TokenStream::new("foo bar");
        let checkpoint = stream.checkpoint();

        drop(stream.peek_token());

        assert_eq!(stream.offset_from(&checkpoint), 0);

        Ok(())
    }

    #[rstest]
    fn consuming_a_peeked_token_reports_its_width() -> Result<(), TestError> {
        let mut stream = TokenStream::new("foo bar");
        let checkpoint = stream.checkpoint();

        drop(stream.peek_token());
        drop(stream.next_token());

        assert_eq!(stream.offset_from(&checkpoint), 3);

        Ok(())
    }
}

mod eof_offset {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn unaffected_by_a_prior_peek() -> Result<(), TestError> {
        let stream = TokenStream::new("foo bar");
        let before = stream.eof_offset();

        drop(stream.peek_token());

        assert_eq!(stream.eof_offset(), before);

        Ok(())
    }
}

mod location {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn current_token_start_skips_whitespace_the_previous_token_end_does_not()
    -> Result<(), TestError> {
        let mut stream = TokenStream::new("foo   bar");

        drop(stream.next_token());

        assert_eq!(stream.previous_span_end(), 3);
        assert_eq!(stream.current_span_start(), 6);

        Ok(())
    }
}

mod checkpoint {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn reset_restores_a_buffered_lookahead() -> Result<(), TestError> {
        let mut stream = TokenStream::new("foo bar");

        drop(stream.peek_token());
        let checkpoint = stream.checkpoint();
        drop(stream.next_token());
        stream.reset(&checkpoint);

        assert_eq!(stream.peek_token(), Some(Token::Word(Cow::Borrowed("foo"))));

        Ok(())
    }
}
