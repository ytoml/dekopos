// Lazy implementation of conversion from String literals to ucs2.

use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};

/// This macro expands string literal to u16 array
#[proc_macro]
pub fn lazy_ucs2(input: TokenStream) -> TokenStream {
    expand(input).enclose()
}

/// This macro expands string literal to u16 array with trailing zero.
#[proc_macro]
pub fn lazy_ucs2z(input: TokenStream) -> TokenStream {
    let mut ret = expand(input);
    ret.push(TokenTree::Literal(Literal::u16_suffixed(0)));
    ret.enclose()
}

fn expand(input: TokenStream) -> Vec<TokenTree> {
    let mut it = input.into_iter();
    let mut ret = vec![];
    // If multiple literals are passed, concatenate them.
    loop {
        match it.next() {
            Some(TokenTree::Literal(l)) => {
                for c in l.confirm_string().chars() {
                    if c as u32 <= u16::MAX as u32 {
                        // constructing array
                        // example: "hey" -> 104u16, 101u16, 121u16
                        // Note that braket will be generated at last.
                        ret.push(TokenTree::Literal(Literal::u16_suffixed(c as u32 as u16)));
                        ret.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
                    } else {
                        panic!("{} is invalid for 16 byte literal", c);
                    }
                }
            }
            Some(TokenTree::Punct(_)) => {}
            Some(tok) => panic!("non-literal passed: {}", tok),
            None => break,
        }
    }
    ret
}

trait ConfirmString: ToString + Sized {
    fn confirm_string(self) -> String;
}

impl ConfirmString for Literal {
    fn confirm_string(self) -> String {
        let literal = self.to_string();
        let mut it = literal.chars();

        if it.next() != Some('\"') {
            panic!("Only string literal is acceptable.");
        }

        let mut ret = String::new();
        loop {
            match it.next() {
                Some('\"') => match it.next() {
                    Some(c) => panic!("malformed literal with trailing {}", c),
                    None => break,
                },
                Some('\\') => match it.next() {
                    Some('\0') => ret.push('\0'),
                    Some('\\') => ret.push('\\'),
                    Some('\'') => ret.push('\''),
                    Some('\"') => ret.push('\"'),
                    Some('r') => ret.push('\r'),
                    Some('n') => ret.push('\n'),
                    Some('t') => ret.push('\t'),
                    Some(c) => todo!("SORRY: Escape sequence {} is not supported now.", c),
                    None => panic!("malformed literal (unexpected EOS)"),
                },
                Some(c) => ret.push(c),
                None => panic!("malformed literal (unexpected EOS)"),
            }
        }
        ret
    }
}

trait Braket: Sized {
    fn enclose(self) -> TokenStream;
}

impl Braket for TokenStream {
    fn enclose(self) -> TokenStream {
        TokenTree::Group(Group::new(Delimiter::Bracket, self)).into()
    }
}

impl Braket for Vec<TokenTree> {
    fn enclose(self) -> TokenStream {
        TokenTree::Group(Group::new(Delimiter::Bracket, self.into_iter().collect())).into()
    }
}
