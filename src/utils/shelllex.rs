// SPDX-License-Identifier: GPL-2.0-or-later

//! This module is responsible for splitting strings into tokens in a way similar
//! to how a shell would do it. For example, the following string:
//! 
//!     --hook key:a exec-shell="echo Hello, world!"
//! 
//! Should be split into three tokens: "--hook", "key:a" and "exec-shell=Hello, world!".
//! Yes, removing the quotes around <<Hello, world!>> is intentional.

use crate::error::ArgumentError;

#[derive(Clone, Copy)]
enum State {
    /// Nothing special about the next character.    
    Normal,
    /// The next character is part of a comment.
    Comment,
    /// The next character is part of a string.
    Quoted(QuoteMark),
}

#[derive(Clone, Copy)]
enum MaybeEscapedState {
    /// We are currently processing the character after a \ character.
    /// After processing it, return to the state contained.
    Escaped(State),
    /// We are not processing the character escaped by a \ character.
    NotEscaped(State),
}

/// Used to distinguish single-quoted strings from double-quoted strings.
#[derive(Clone, Copy)]
enum QuoteMark {
    Single,
    Double,
}

impl QuoteMark {
    fn as_char(&self) -> char {
        match self {
            QuoteMark::Single => '\'',
            QuoteMark::Double => '\"',
        }
    }

    fn try_from(character: char) -> Option<QuoteMark> {
        match character {
            '\'' => Some(QuoteMark::Single),
            '\"' => Some(QuoteMark::Double),
            _ => None,
        }
    }
}

// TODO: FEATURE(config) Should we also interpret the `# ... ` sequence, since we use it in
// quite a lot of our own examples?
// TODO: FEATURE(config) Should we treat \r\n the same as we treat \n?

/// Tries to split a string into tokens in a way similar to how a shell does it.
pub fn lex(input: &str) -> Result<Vec<String>, ArgumentError> {
    let mut state = MaybeEscapedState::NotEscaped(State::Normal);
    let mut next_token: Option<String> = None;
    let mut tokens: Vec<String> = Vec::new();

    // Read characters from the input and append them to next_token.
    //
    // Unless the character is whitespace, in which case the current token shall be
    // finalized, meaning that future characters must be written to a new token.
    //
    // Unless some control character gets encountered, in which case the state is modified
    // according to that control character.
    //
    // Unless...
    //
    // You get the gist. I can't summarize the next 80 lines in a comment.
    for character in input.chars() {
        match state {
            // Handle generic characters that are not under any special mode of processing.
            MaybeEscapedState::NotEscaped(State::Normal) => {
                match character {
                    '#' => {
                        finalize_token(&mut tokens, &mut next_token);
                        state = MaybeEscapedState::NotEscaped(State::Comment);
                    },
                    '\\' => {
                        state = MaybeEscapedState::Escaped(State::Normal);
                    },
                    '\'' | '\"' => {
                        if next_token.is_none() {
                            next_token = Some(String::new());
                        }
                        state = MaybeEscapedState::NotEscaped(State::Quoted(
                            QuoteMark::try_from(character).unwrap()
                        ));
                    },
                    _ if character.is_ascii_whitespace() => {
                        finalize_token(&mut tokens, &mut next_token);
                    },
                    _ => {
                        push_to_token(&mut next_token, character);
                    },
                }
            },

            // Handle characters in comments.
            MaybeEscapedState::NotEscaped(State::Comment) => {
                if character == '\n' {
                    state = MaybeEscapedState::NotEscaped(State::Normal);
                } else {
                    // Ignore character.
                }
            },

            // Handle characters inside a string.
            MaybeEscapedState::NotEscaped(quote_state @ State::Quoted(used_quote_mark)) => {
                match character {
                    mark if mark == used_quote_mark.as_char() => {
                        state = MaybeEscapedState::NotEscaped(State::Normal);
                    },
                    '\\' => {
                        state = MaybeEscapedState::Escaped(quote_state);
                    },
                    _ => {
                        push_to_token(&mut next_token, character);
                    }
                } 
            },

            // Handle escaped characters after a backslash (\) character.
            MaybeEscapedState::Escaped(last_state) => {
                // A backslash before a newline causes that newline to be ignored.
                if character == '\n' {
                    state = MaybeEscapedState::NotEscaped(last_state);
                    continue;
                }

                // TODO: Expand the following list.
                let mapped_char = match character {
                    'n'  => '\n',
                    'r'  => '\r',
                    't'  => '\t',
                    '\\' => '\\',
                    '\'' => '\'',
                    '`'  => '`',
                    '\"' => '\"',
                    '#'  => '#',
                    '*'  => '*',
                    '?'  => '?',
                    ' '  => ' ',
                    _ => return Err(ArgumentError::new(format!(
                        "Unknown escape sequence encountered: \\{}", character
                    ))),
                };

                push_to_token(&mut next_token, mapped_char);
                state = MaybeEscapedState::NotEscaped(last_state);
            }
        }
    }

    // All characters have been read. Make sure we are in a valid state now.
    match state {
        MaybeEscapedState::Escaped(_) => {
            return Err(ArgumentError::new("Encountered an escape character (\\) at end of stream."));
        },
        MaybeEscapedState::NotEscaped(State::Quoted(quote_char)) => {
            return Err(ArgumentError::new(format!(
                "Reached end-of-stream before finding the end of a string: {}{}",
                quote_char.as_char(),
                next_token.unwrap_or_default(),
            )));
        }
        MaybeEscapedState::NotEscaped(State::Normal | State::Comment) => {
            finalize_token(&mut tokens, &mut next_token);
        }
    }

    Ok(tokens)
}

/// Adds a character to the token that is currently being accumulated. Creates a new
/// token if no token is currently being accumulated.
fn push_to_token(token: &mut Option<String>, character: char) {
    match token {
        Some(string) => string.push(character),
        None => {
            *token = Some(character.into());
        },
    }
}

/// If a token was getting accumulated, finalize it in the sense that no new character
/// can be added to it anymore. Sets the currently accumulating token to empty, ensuring
/// that all future characters will be added to a new token.
fn finalize_token(tokens: &mut Vec<String>, token: &mut Option<String>) {
    if let Some(item) = token.take() {
        tokens.push(item);
    }
}

// TODO: Write some more unittests.
#[test]
fn unittest() {
    assert_eq!(
        lex("--hook exec-shell=\"echo Hello, world!\"").unwrap(),
        vec!["--hook".to_owned(), "exec-shell=echo Hello, world!".to_owned()]
    );
    assert_eq!(
        lex("foo \"bar\" 'baz' \"q\"u'u'\"x\"").unwrap(),
        vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned(), "quux".to_owned()],
    );
    assert_eq!(
        lex("foo bar # baz \nquux").unwrap(),
        vec!["foo".to_owned(), "bar".to_owned(), "quux".to_owned()],
    );
    assert_eq!(
        lex("foo bar # baz \nquux").unwrap(),
        vec!["foo".to_owned(), "bar".to_owned(), "quux".to_owned()],
    );
    assert_eq!(
        lex("a b\\ c").unwrap(),
        vec!["a".to_owned(), "b c".to_owned()],
    );
    assert_eq!(
        lex("foo \"bar 'baz' \\\"quux\\\"\"").unwrap(),
        vec!["foo".to_owned(), "bar 'baz' \"quux\"".to_owned()],
    );
    assert_eq!(
        lex("foo # \\\nbar").unwrap(),
        vec!["foo".to_owned(), "bar".to_owned()],
    );
    assert_eq!(
        lex("foo\\\nbar").unwrap(),
        vec!["foobar".to_owned()],
    );
    assert_eq!(
        lex("").unwrap(),
        Vec::<String>::new(),
    );
    assert_eq!(
        lex(" ").unwrap(),
        Vec::<String>::new(),
    );
    assert_eq!(
        lex("\t\n").unwrap(),
        Vec::<String>::new(),
    );
    assert_eq!(
        lex("\"\"").unwrap(),
        vec!["".to_owned()],
    );
    assert_eq!(
        lex("--map \"\" key:a").unwrap(),
        vec!["--map".to_owned(), "".to_owned(), "key:a".to_owned()],
    );
    assert_eq!(
        lex("   foo    ").unwrap(),
        vec!["foo".to_owned()],
    );
    assert_eq!(
        lex("   foo  \\  ").unwrap(),
        vec!["foo".to_owned(), " ".to_owned()],
    );
    assert_eq!(
        lex("foo \" # \" '#' \\# bar # baz").unwrap(),
        vec!["foo".to_owned(), " # ".to_owned(), "#".to_owned(), "#".to_owned(), "bar".to_owned()],
    );
    

    lex("foo \"bar").unwrap_err();
    lex("foo \\").unwrap_err();
    lex("foo \"'").unwrap_err();
}
