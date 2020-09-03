use crate::{BencodeAny, Token, TokenType};

use std::cell::Cell;
use std::iter::FusedIterator;

/// Iterator over `BencodeList` items
#[derive(Debug, Clone)]
pub struct BencodeListIter<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into root_tokens that the iterator is currently at
    token_idx: usize,
    /// The number of times this iterator's `next()` method has returned
    /// `Some(_)`.
    num_traversed: u32,
    /// If this is `Some(size)` knew the size of this list before we created
    /// the iterator.
    precalculated_size: Option<u32>,
}

impl<'a, 't> BencodeListIter<'a, 't> {
    pub(super) fn new(
        buf: &'a [u8],
        root_tokens: &'t [Token],
        token_idx: usize,
        precalculated_size: Option<u32>,
    ) -> Self {
        Self {
            buf,
            root_tokens,
            token_idx,
            num_traversed: 0,
            precalculated_size,
        }
    }

    fn create_any_at_current_pos(&self) -> BencodeAny<'a, 't> {
        BencodeAny {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: self.token_idx,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

impl<'a, 't> FusedIterator for BencodeListIter<'a, 't> {}

impl<'a, 't> Iterator for BencodeListIter<'a, 't> {
    type Item = BencodeAny<'a, 't>;

    fn next(&mut self) -> Option<BencodeAny<'a, 't>> {
        if self.root_tokens[self.token_idx].token_type() == TokenType::End {
            None
        } else {
            let result = self.create_any_at_current_pos();
            self.token_idx += self.root_tokens[self.token_idx].next_item();
            self.num_traversed += 1;
            Some(result)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.precalculated_size {
            Some(size) => {
                debug_assert!(self.num_traversed <= size);
                ((size - self.num_traversed) as usize, Some(size as usize))
            }
            None => (0, None),
        }
    }
}

/// Iterator over `BencodeDict` keys and value tuples
#[derive(Debug, Clone)]
pub struct BencodeDictIter<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into root_tokens that the iterator is currently at
    token_idx: usize,
    /// The number of times this iterator's `next()` method has returned
    /// `Some(_)`.
    num_traversed: u32,
    /// If this is `Some(size)` knew the size of this dictionary before we
    /// created the iterator.
    precalculated_size: Option<u32>,
}

impl<'a, 't> BencodeDictIter<'a, 't> {
    pub(super) fn new(
        buf: &'a [u8],
        root_tokens: &'t [Token],
        token_idx: usize,
        precalculated_size: Option<u32>,
    ) -> Self {
        Self {
            buf,
            root_tokens,
            token_idx,
            num_traversed: 0,
            precalculated_size,
        }
    }

    fn create_any(&self, index: usize) -> BencodeAny<'a, 't> {
        BencodeAny {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: index,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

impl<'a, 't> FusedIterator for BencodeDictIter<'a, 't> {}

impl<'a, 't> Iterator for BencodeDictIter<'a, 't> {
    type Item = (&'a [u8], BencodeAny<'a, 't>);

    fn next(&mut self) -> Option<(&'a [u8], BencodeAny<'a, 't>)> {
        if self.root_tokens[self.token_idx].token_type() == TokenType::End {
            None
        } else {
            let key_node = self.create_any(self.token_idx);
            let key = key_node.as_string().unwrap().as_bytes();

            let value_token = self.token_idx + self.root_tokens[self.token_idx].next_item();
            let value_node = self.create_any(value_token);

            self.token_idx = value_token + self.root_tokens[value_token].next_item();
            self.num_traversed += 1;
            Some((key, value_node))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.precalculated_size {
            Some(size) => {
                debug_assert!(self.num_traversed <= size);
                ((size - self.num_traversed) as usize, Some(size as usize))
            }
            None => (0, None),
        }
    }
}
