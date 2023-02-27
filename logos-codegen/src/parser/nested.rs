use proc_macro2::token_stream::IntoIter as TokenIter;
use proc_macro2::{Ident, Literal, TokenStream as TokenStream2, TokenTree};

use proc_macro::TokenStream;
use quote::quote;

use crate::parse::prelude::*;
use crate::util::{expect_punct, is_punct};

pub struct CommaSplitter {
    stream: ParseStream,
}

impl CommaSplitter {
    pub fn new(stream: TokenStream2) -> Self {
        CommaSplitter {
            stream: stream.parse_stream(),
        }
    }
}

impl Iterator for CommaSplitter {
    type Item = TokenStream;

    fn next(&mut self) -> Option<TokenStream> {
        let first = self.stream.next()?;

        if first.is(',') {
            return Some(TokenStream::new());
        }

        let mut out: TokenStream = first.into();

        out.extend((&mut self.stream).take_while(|tt| !tt.is(',')));

        Some(out)
    }
}

/// `name = ...`
pub struct NestedAssign<T = TokenStream> {
    pub value: T,
}

/// `name ident = ...`
pub struct NestedKeywordAssign<T = TokenStream> {
    pub name: proc_macro::Ident,
    pub value: T,
}

impl<T: Parse> Parse for NestedAssign<T> {
    fn parse(stream: &mut ParseStream) -> Result<Self, ParseError> {
        stream.expect('=')?;

        Ok(NestedAssign {
            value: stream.parse()?,
        })
    }
}

impl<T: Parse> Parse for NestedKeywordAssign<T> {
    fn parse(stream: &mut ParseStream) -> Result<Self, ParseError> {
        let name = stream.parse()?;

        stream.expect('=')?;

        Ok(NestedKeywordAssign {
            name,
            value: stream.parse()?,
        })
    }
}

pub enum NestedValue {
    /// `name = ...`
    Assign(TokenStream2),
    /// `name "literal"`
    Literal(Literal),
    /// `name(...)`
    Group(TokenStream2),
    /// `name ident = ...`
    KeywordAssign(Ident, TokenStream2),
}

pub enum Nested {
    /// Unnamed nested attribute, such as a string,
    /// callback closure, or a lone ident/path
    ///
    /// Note: a lone ident will be Named with no value instead
    Unnamed(TokenStream2),
    /// Named: name ...
    Named(Ident, NestedValue),
    /// Unexpected token,
    Unexpected(TokenStream2),
}

pub struct AttributeParser {
    inner: TokenIter,
}

pub struct Empty;

impl From<Empty> for TokenStream2 {
    fn from(_: Empty) -> TokenStream2 {
        TokenStream2::new()
    }
}

impl AttributeParser {
    pub fn new(stream: TokenStream2) -> Self {
        AttributeParser {
            inner: stream.into_iter(),
        }
    }

    pub fn parsed<T>(&mut self) -> Option<syn::Result<T>>
    where
        T: syn::parse::Parse,
    {
        let tokens = self.collect_tail(TokenStream2::new());

        if tokens.is_empty() {
            return None;
        }

        Some(syn::parse2(tokens))
    }

    fn next_tt(&mut self) -> Option<TokenTree> {
        expect_punct(self.inner.next(), ',')
    }

    fn collect_tail<T>(&mut self, first: T) -> TokenStream2
    where
        T: Into<TokenStream2>,
    {
        let mut out = first.into();

        while let Some(tt) = self.next_tt() {
            out.extend(Some(tt));
        }

        out
    }

    fn parse_unnamed(&mut self, first: Ident, next: TokenTree) -> Nested {
        let mut out = TokenStream2::from(TokenTree::Ident(first));

        out.extend(self.collect_tail(next));

        Nested::Unnamed(out.into_iter().collect())
    }

    fn parse_assign(&mut self, name: Ident) -> Nested {
        let value = self.collect_tail(Empty);

        Nested::Named(name, NestedValue::Assign(value))
    }

    fn parse_literal(&mut self, name: Ident, lit: Literal) -> Nested {
        // TODO: Error if there are any tokens following
        let _ = self.collect_tail(Empty);

        Nested::Named(name, NestedValue::Literal(lit))
    }

    fn parse_group(&mut self, name: Ident, group: TokenStream2) -> Nested {
        Nested::Named(name, NestedValue::Group(group))
    }

    fn parse_keyword(&mut self, keyword: Ident, name: Ident) -> Nested {
        let error = expect_punct(self.next_tt(), '=');

        match error {
            Some(error) => {
                let error = self.collect_tail(error);

                Nested::Unexpected(error)
            }
            None => {
                let value = self.collect_tail(Empty);

                Nested::Named(keyword, NestedValue::KeywordAssign(name, value))
            }
        }
    }
}

impl Iterator for AttributeParser {
    type Item = Nested;

    fn next(&mut self) -> Option<Nested> {
        let first = self.inner.next()?;

        let name = match first {
            TokenTree::Ident(ident) => ident,
            tt => {
                let stream = self.collect_tail(tt);

                return Some(Nested::Unnamed(stream.into_iter().collect()));
            }
        };

        match self.next_tt() {
            Some(tt) if is_punct(&tt, '=') => Some(self.parse_assign(name)),
            Some(TokenTree::Literal(lit)) => Some(self.parse_literal(name, lit)),
            Some(TokenTree::Group(group)) => Some(self.parse_group(name, group.stream())),
            Some(TokenTree::Ident(next)) => Some(self.parse_keyword(name, next)),
            Some(next) => Some(self.parse_unnamed(name, next)),
            None => Some(Nested::Unnamed(quote!(#name))),
        }
    }
}
