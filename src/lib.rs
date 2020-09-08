//! A Bencode decoder in Rust.
#![deny(
    missing_docs,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_qualifications
)]
#![deny(
    clippy::correctness,
    clippy::style,
    clippy::perf,
)]


mod iterators;
mod parse_int;
mod stack_frame;
mod token;

use memchr::memchr;

use iterators::{BencodeDictIter, BencodeListIter};
use parse_int::{check_integer, decode_int, is_numeric};
use stack_frame::{StackFrame, StackFrameState};
use token::{Token, TokenType};

use std::cell::Cell;
use std::convert::TryInto;
use std::fmt;

/// Error which can occur when calling `bdecode()`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BdecodeError {
    /// Expected digit in bencoded string
    ExpectedDigit,
    /// Expected colon in bencoded string
    ExpectedColon,
    /// Unexpected end of file in bencoded string
    UnexpectedEof,
    /// Expected value (list, dict, int, or string) in bencoded string
    ExpectedValue,
    /// Bencoded recursion depth limit exceeded
    DepthExceeded,
    /// Bencoded item count limit exceeded
    LimitExceeded,
    /// Integer overflow
    Overflow,
    /// Leading zero in integer
    LeadingZero,
    /// Integer is negative zero
    NegativeZero,
}

/// The type of a node
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NodeType {
    /// This node is a dictionary
    Dict,
    /// This node is a list
    List,
    /// This node is a string
    Str,
    /// This node is a integer
    Int,
}

#[derive(Clone)]
/// Struct which owns the bencode tokens. Call `get_root()` to receive a
/// handle for the root object.
pub struct Bencode<'a> {
    buf: &'a [u8],
    tokens: Vec<Token>,
}

impl<'a> fmt::Debug for Bencode<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bencode")
            .field("content", &self.get_root())
            .finish()
    }
}

impl<'a> Bencode<'a> {
    /// Returns a handle on the root object.
    pub fn get_root<'t>(&'t self) -> BencodeAny<'a, 't> {
        BencodeAny {
            buf: self.buf,
            root_tokens: &self.tokens,
            token_idx: 0,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

/// A bencoded list
#[derive(Clone)]
pub struct BencodeList<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into m_root_tokens that this node refers to
    /// for the root node, it's 0.
    token_idx: usize,
    /// this is a cache of the last element index looked up. This only applies
    /// to lists and dictionaries. If the next lookup is at m_last_index or
    /// greater, we can start iterating the tokens at m_last_token.
    cached_lookup: Cell<Option<(usize, usize)>>,
    /// the number of elements in this list or dict (computed on the first
    /// call to dict_size() or list_size())
    cached_size: Cell<Option<usize>>,
}

impl<'a, 't> BencodeList<'a, 't> {
    /// Returns the item in the list at the given index.
    pub fn get(&self, index: usize) -> Option<BencodeAny<'a, 't>> {
        let mut token = self.token_idx + 1;
        let mut item = 0;

        if self.root_tokens[token].token_type() == TokenType::End {
            // index out of range
            self.cached_size.set(Some(item));
            return None;
        }

        let lookup = self.cached_lookup.get();
        if let Some((last_token, last_index)) = lookup {
            if last_index >= index {
                token = last_token;
                item = last_index;
            }
        }

        while item < index {
            token += self.root_tokens[token].next_item();
            item += 1;
            // index out of range
            if self.root_tokens[token].token_type() == TokenType::End {
                // at least we know the size of the list now :p
                self.cached_size.set(Some(item));
                return None;
            }
        }

        // There's no point in caching the first item
        if index > 0 {
            self.cached_lookup.set(Some((token, index)));
        }

        Some(self.create_any(token))
    }

    /// Returns how many items there are in this list.
    pub fn len(&self) -> usize {
        // Maybe we have the size cached
        if let Some(size) = self.cached_size.get() {
            return size;
        }

        let mut token = self.token_idx + 1;
        let mut size = 0;

        if let Some((last_token, last_index)) = self.cached_lookup.get() {
            token = last_token;
            size = last_index;
        }

        while self.root_tokens[token].token_type() != TokenType::End {
            token += self.root_tokens[token].next_item();
            size += 1;
        }

        self.cached_size.set(Some(size));
        size
    }

    /// Returns true if the length of this list is zero.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the list's items.
    pub fn iter(&self) -> BencodeListIter<'a, 't> {
        BencodeListIter::new(
            self.buf,
            self.root_tokens,
            self.token_idx + 1,
            self.cached_size.get().map(|size| size as u32),
        )
    }

    fn create_any(&self, token_idx: usize) -> BencodeAny<'a, 't> {
        BencodeAny {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

impl<'a, 't> fmt::Debug for BencodeList<'a, 't> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A bencoded dictionary
#[derive(Clone)]
pub struct BencodeDict<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into m_root_tokens that this node refers to
    /// for the root node, it's 0.
    token_idx: usize,
    /// this is a cache of the last element index looked up. This only applies
    /// to lists and dictionaries. If the next lookup is at m_last_index or
    /// greater, we can start iterating the tokens at m_last_token.
    cached_lookup: Cell<Option<(usize, usize)>>,
    /// the number of elements in this list or dict (computed on the first
    /// call to dict_size() or list_size())
    cached_size: Cell<Option<usize>>,
}

impl<'a, 't> BencodeDict<'a, 't> {
    /// Get the key-value pair at the given index. Returns `None` if index is
    /// out of bounds.
    pub fn get(&self, index: usize) -> Option<(&'a [u8], BencodeAny<'a, 't>)> {
        let mut token = self.token_idx + 1;
        let mut item = 0;

        if self.root_tokens[token].token_type() == TokenType::End {
            // index out of range
            self.cached_size.set(Some(item));
            return None;
        }

        // do we have a lookup cached?
        if let Some((last_token, last_index)) = self.cached_lookup.get() {
            if last_index >= index {
                token = last_token;
                item = last_index;
            }
        }

        while item < index {
            assert_eq!(self.root_tokens[token].token_type(), TokenType::Str);

            // skip the key
            token += self.root_tokens[token].next_item();
            if self.root_tokens[token].token_type() == TokenType::End {
                // index out of range
                self.cached_size.set(Some(item));
                return None;
            }
            // skip the value
            token += self.root_tokens[token].next_item();
            if self.root_tokens[token].token_type() == TokenType::End {
                // index out of range
                self.cached_size.set(Some(item));
                return None;
            }
            item += 1;
        }

        // There's no point in caching the first item
        if index > 0 {
            self.cached_lookup.set(Some((token, index)));
        }

        let key_node = self.create_any(token);
        // The key is always a string, so we can unwrap here
        let key = key_node.as_string().unwrap().as_bytes();

        let value_token = token + self.root_tokens[token].next_item();
        let value_node = self.create_any(value_token);

        Some((key, value_node))
    }

    /// Get the value corresponding to the given key. Returns `None` if index
    /// is out of bounds.
    pub fn find(&self, key: &[u8]) -> Option<BencodeAny<'a, 't>> {
        let mut token = self.token_idx + 1;

        while self.root_tokens[token].token_type() != TokenType::End {
            let t = &self.root_tokens[token];
            // the keys should always be strings
            assert_eq!(t.token_type(), TokenType::Str);
            let t_off = t.offset();
            let t_off_start = t.start_offset();

            let t_next = &self.root_tokens[token + 1];
            let t_next_off = t_next.offset();

            // compare the keys
            let size = t_next_off - t_off - t_off_start;
            if (size == key.len())
                && (key == &self.buf[(t_off + t_off_start)..(t_off + t_off_start + size)])
            {
                // skip key
                token += t.next_item();
                assert_ne!(self.root_tokens[token].token_type(), TokenType::End);
                // return the value
                return Some(BencodeAny {
                    buf: self.buf,
                    root_tokens: self.root_tokens,
                    token_idx: token,
                    cached_lookup: Cell::new(None),
                    size: Cell::new(None),
                });
            }
            // skip key
            token += t.next_item();
            assert_ne!(self.root_tokens[token].token_type(), TokenType::End);
            // skip value
            token += self.root_tokens[token].next_item();
        }

        None
    }

    /// Returns how many items there are in this dictionary.
    pub fn len(&self) -> usize {
        // Maybe we have the size cached
        if let Some(size) = self.cached_size.get() {
            return size;
        }

        let mut token = self.token_idx + 1;
        let mut item = 0;

        if let Some((last_token, last_index)) = self.cached_lookup.get() {
            token = last_token;
            item = last_index * 2;
        }

        while self.root_tokens[token].token_type() != TokenType::End {
            token += self.root_tokens[token].next_item();
            item += 1;
        }

        // a dictionary must contain full key-value pairs. which means
        // the number of entries is divisible by 2
        assert_eq!(item % 2, 0);

        // each item is one key and one value, so divide by 2
        let size = item / 2;

        self.cached_size.set(Some(size));
        size
    }

    /// Returns true if the length of this dictionary is zero.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the key-value pairs in this dictionary.
    pub fn iter(&self) -> BencodeDictIter<'a, 't> {
        BencodeDictIter::new(
            self.buf,
            self.root_tokens,
            self.token_idx + 1,
            self.cached_size.get().map(|size| size as u32),
        )
    }

    fn create_any(&self, token_idx: usize) -> BencodeAny<'a, 't> {
        BencodeAny {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

impl<'a, 't> fmt::Debug for BencodeDict<'a, 't> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// A bencoded integer
#[derive(Clone)]
pub struct BencodeInt<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into m_root_tokens that this node refers to
    /// for the root node, it's 0.
    token_idx: usize,
}

impl<'a, 't> BencodeInt<'a, 't> {
    /// Returns a slice into the original input buffer of the bytes that make
    /// up this integer.
    pub fn as_bytes(&self) -> &'a [u8] {
        let t = &self.root_tokens[self.token_idx];
        let t_off = t.offset();
        debug_assert_eq!(self.buf[t_off], b'i');

        let t_next = &self.root_tokens[self.token_idx + 1];
        let t_next_off = t_next.offset();

        // Minus `2` to exclude the `e` character, and the first character of
        // the next token.
        debug_assert_eq!(self.buf[t_next_off - 1], b'e');
        let size = t_next_off - 2 - t_off;

        let int_start = t_off + 1;
        &self.buf[int_start..(int_start + size)]
    }

    /// Get the integer value as an `i64`. This will be depricated in favour
    /// of the `From` trait.
    pub fn value(&self) -> Result<i64, BdecodeError> {
        Ok(decode_int(self.as_bytes())?)
    }
}

impl<'a, 't> fmt::Debug for BencodeInt<'a, 't> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(std::str::from_utf8(self.as_bytes()).unwrap())
    }
}

/// A bencoded string
#[derive(Clone)]
pub struct BencodeString<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into m_root_tokens that this node refers to
    /// for the root node, it's 0.
    token_idx: usize,
}

impl<'a, 't> BencodeString<'a, 't> {
    /// Returns a slice into the original input buffer of the bytes that make
    /// up this string.
    pub fn as_bytes(&self) -> &'a [u8] {
        let t = &self.root_tokens[self.token_idx];
        let t_off = t.offset();
        let t_off_start = t.start_offset();

        let t_next = &self.root_tokens[self.token_idx + 1];
        let t_next_off = t_next.offset();

        let size = t_next_off - t_off - t_off_start;

        &self.buf[(t_off + t_off_start)..(t_off + t_off_start + size)]
    }
}

impl<'a, 't> fmt::Debug for BencodeString<'a, 't> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("BencodeString({:?})", self.as_bytes()))
    }
}

/// A bencoded object which could be of any type. You probably want to call
/// one of `as_list()`, `as_dict()`, `as_int()`, `as_string()` to convert this
/// struct into a concrete type.
#[derive(Clone)]
pub struct BencodeAny<'a, 't> {
    buf: &'a [u8],
    /// this points to the root node's token vector
    /// for the root node, this points to its own tokens member
    root_tokens: &'t [Token],
    /// this is the index into m_root_tokens that this node refers to
    /// for the root node, it's 0.
    token_idx: usize,
    /// this is a cache of the last element index looked up. This only applies
    /// to lists and dictionaries. If the next lookup is at m_last_index or
    /// greater, we can start iterating the tokens at m_last_token.
    cached_lookup: Cell<Option<(usize, usize)>>,
    /// the number of elements in this list or dict (computed on the first
    /// call to dict_size() or list_size())
    size: Cell<Option<usize>>,
}

impl<'a, 't> fmt::Debug for BencodeAny<'a, 't> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.node_type() {
            NodeType::Dict => {
                let self_dict = self.as_dict().unwrap();
                self_dict.fmt(f)
            }
            NodeType::List => {
                let self_list = self.as_list().unwrap();
                self_list.fmt(f)
            }
            NodeType::Int => {
                let self_int = self.as_int().unwrap();
                self_int.fmt(f)
            }
            NodeType::Str => {
                let self_str = self.as_string().unwrap();
                self_str.fmt(f)
            }
        }
    }
}

impl<'a, 't> BencodeAny<'a, 't> {
    /// The type of the bencoded object.
    pub fn node_type(&self) -> NodeType {
        let token_type = self.root_tokens[self.token_idx].token_type();
        match token_type {
            TokenType::Dict => NodeType::Dict,
            TokenType::List => NodeType::List,
            TokenType::Int => NodeType::Int,
            TokenType::Str => NodeType::Str,
            _ => unreachable!("{:?} unexpected", token_type),
        }
    }

    /// Try to convert this struct into a `BencodeList`. This fails if and
    /// only if the underlying bencoded object is not a list.
    pub fn as_list(&self) -> Option<BencodeList<'a, 't>> {
        if self.node_type() != NodeType::List {
            return None;
        }
        Some(BencodeList {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: self.token_idx,
            cached_lookup: Cell::new(None),
            cached_size: Cell::new(None),
        })
    }

    /// Try to convert this struct into a `BencodeDict`. This fails if and
    /// only if the underlying bencoded object is not a dictionary.
    pub fn as_dict(&self) -> Option<BencodeDict<'a, 't>> {
        if self.node_type() != NodeType::Dict {
            return None;
        }
        Some(BencodeDict {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: self.token_idx,
            cached_lookup: Cell::new(None),
            cached_size: Cell::new(None),
        })
    }

    /// Try to convert this struct into a `BencodeInt`. This fails if and
    /// only if the underlying bencoded object is not an integer.
    pub fn as_int(&self) -> Option<BencodeInt<'a, 't>> {
        if self.node_type() != NodeType::Int {
            return None;
        }
        Some(BencodeInt {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: self.token_idx,
        })
    }

    /// Try to convert this struct into a `BencodeString`. This fails if and
    /// only if the underlying bencoded object is not a string.
    pub fn as_string(&self) -> Option<BencodeString<'a, 't>> {
        if self.node_type() != NodeType::Str {
            return None;
        }
        Some(BencodeString {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: self.token_idx,
        })
    }
}

/// Decode a bencoded buffer into a `Bencode` struct.
pub fn bdecode(buf: &[u8]) -> Result<Bencode<'_>, BdecodeError> {
    if buf.len() > Token::MAX_OFFSET {
        return Err(BdecodeError::LimitExceeded);
    }
    if buf.is_empty() {
        return Err(BdecodeError::UnexpectedEof);
    }
    let mut sp: usize = 0;
    let mut stack: Vec<StackFrame> = Vec::with_capacity(4);
    let mut tokens: Vec<Token> = Vec::with_capacity(16);
    let mut off = 0;
    while off < buf.len() {
        let byte = buf[off];
        let current_frame = sp;

        // if we're currently parsing a dictionary, assert that
        // every other node is a string.
        if (current_frame > 0)
            && tokens[stack[current_frame - 1].token()].token_type() == TokenType::Dict
            && stack[current_frame - 1].state() == StackFrameState::Key
        {
            // the current parent is a dict and we are parsing a key.
            // only allow a digit (for a string) or 'e' to terminate
            if !is_numeric(byte) && byte != b'e' {
                return Err(BdecodeError::ExpectedDigit);
            }
        }

        match byte {
            b'd' => {
                let new_frame =
                    StackFrame::new(tokens.len().try_into().unwrap(), StackFrameState::Key);
                stack.push(new_frame);
                sp += 1;
                // we push it into the stack so that we know where to fill
                // in the next_node field once we pop this node off the stack.
                // i.e. get to the node following the dictionary in the buffer
                let new_token = Token::new(off, TokenType::Dict, 0, 0)?;
                tokens.push(new_token);
                off += 1;
            }
            b'l' => {
                let new_frame =
                    StackFrame::new(tokens.len().try_into().unwrap(), StackFrameState::Key);
                stack.push(new_frame);
                sp += 1;
                // we push it into the stack so that we know where to fill
                // in the next_node field once we pop this node off the stack.
                // i.e. get to the node following the list in the buffer
                let new_token = Token::new(off, TokenType::List, 0, 0)?;
                tokens.push(new_token);
                off += 1;
            }
            b'i' => {
                let end_index = match memchr(b'e', &buf[off..]) {
                    Some(idx) => off + idx,
                    None => {
                        return Err(BdecodeError::UnexpectedEof);
                    }
                };
                // +1 here to point to the first digit, rather than 'i'
                check_integer(&buf[(off + 1)..end_index])?;
                let new_token = Token::new(off, TokenType::Int, 1, 1)?;
                tokens.push(new_token);
                debug_assert_eq!(buf[end_index], b'e');
                off = end_index + 1;
            }
            b'e' => {
                // end of list or dict
                if sp == 0 {
                    return Err(BdecodeError::UnexpectedEof);
                }
                if sp > 0
                    && (tokens[stack[sp - 1].token()].token_type() == TokenType::Dict)
                    && stack[sp - 1].state() == StackFrameState::Value
                {
                    // this means we're parsing a dictionary and about to parse a
                    // value associated with a key. Instead, we got a termination
                    return Err(BdecodeError::ExpectedValue);
                }
                // insert end-of-sequence token
                let end_token = Token::new(off, TokenType::End, 1, 0)?;
                tokens.push(end_token);
                // and back-patch the start of this sequence with the offset
                // to the next token we'll insert
                let top = stack[sp - 1].token();
                // subtract the token's own index, since this is a relative
                // offset
                let next_item = tokens.len() - top;
                tokens[top].set_next_item(next_item)?;
                // and pop it from the stack.
                debug_assert!(sp > 0);
                sp -= 1;
                off += 1;
            }
            _ => {
                let str_off = off;
                // this is the case for strings.
                let colon_index = match memchr(b':', &buf[off..]) {
                    Some(idx) => off + idx,
                    None => {
                        return Err(BdecodeError::ExpectedColon);
                    }
                };
                debug_assert_eq!(buf[colon_index], b':');
                let int_buf = &buf[off..colon_index];
                check_integer(int_buf)?;
                let string_length: usize = decode_int(int_buf)?
                    .try_into()
                    .map_err(|_| BdecodeError::Overflow)?;
                // FIXME: Is this needed in my code?
                off = colon_index + 1;
                if off >= buf.len() {
                    return Err(BdecodeError::UnexpectedEof);
                }
                // remaining buffer size
                let remaining = buf.len() - off;
                if string_length > remaining {
                    // The remaining buffer size is not big enough to fit a
                    // string that big.
                    return Err(BdecodeError::UnexpectedEof);
                }

                let header_len = off - str_off - 2;
                let new_token = Token::new(str_off, TokenType::Str, 1, header_len)?;
                tokens.push(new_token);
                off += string_length;
            }
        };

        if current_frame > 0
            && tokens[stack[current_frame - 1].token()].token_type() == TokenType::Dict
        {
            // the next item we parse is the opposite
            stack[current_frame - 1].toggle_state();
        }

        if sp < current_frame {
            // this is a deviation from libtorrent. we do this because we use
            // a dynamically sized vector for tokens, instead of allocating
            // space for the entire stack upfront.
            //
            // if we popped the stack above where we decrement the sp index,
            // we'd end up trying to read out of bounds in the if statement above
            stack.pop();
        }

        if sp == 0 {
            // this terminates the top level node, we're done!
            break;
        }
    }

    if sp > 0 {
        return Err(BdecodeError::UnexpectedEof);
    }

    // one final end token
    tokens.push(Token::new(off, TokenType::End, 0, 0)?);

    Ok(Bencode { buf, tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dict_list_no_end() {
        let result_dict = bdecode(b"d");
        assert!(result_dict.is_err());
        let result_list = bdecode(b"l");
        assert!(result_list.is_err());
    }

    #[test]
    fn test_index_empty_dict() {
        let bencode = bdecode(b"de").unwrap();
        let dict_node = bencode.get_root();
        assert_eq!(dict_node.node_type(), NodeType::Dict);
        assert!(dict_node.as_dict().unwrap().get(0).is_none());
        assert!(dict_node.as_dict().unwrap().find(b"my_key").is_none());
    }

    #[test]
    fn test_index_empty_list() {
        let bencode = bdecode(b"le").unwrap();
        let list_node = bencode.get_root();
        assert_eq!(list_node.node_type(), NodeType::List);
        assert!(list_node.as_list().unwrap().get(0).is_none());
    }

    #[test]
    fn test_list_1() {
        let bencode = bdecode(b"l4:spami42ee").unwrap();
        let root_node = bencode.get_root();
        assert_eq!(root_node.node_type(), NodeType::List);
        assert_eq!(root_node.as_list().unwrap().len(), 2);

        // First element is the string `spam`.
        let elem_0 = root_node.as_list().unwrap().get(0).unwrap();
        assert_eq!(elem_0.node_type(), NodeType::Str);
        assert_eq!(elem_0.as_string().unwrap().as_bytes(), b"spam");

        // The second element is the integer `42`.
        let elem_1 = root_node.as_list().unwrap().get(1).unwrap();
        assert_eq!(elem_1.node_type(), NodeType::Int);

        // the list is only of size 2, so this should be out of bounds
        assert!(root_node.as_list().unwrap().get(2).is_none());
    }

    #[test]
    fn test_dict_1() {
        // Corresponds to the following JSON: {"a":{"b":1,"c":"abcd"},"d":3}
        let bencode = bdecode(b"d1:ad1:bi1e1:c4:abcde1:di3ee").unwrap();
        let root_node = bencode.get_root();
        assert_eq!(root_node.node_type(), NodeType::Dict);
        assert_eq!(root_node.as_dict().unwrap().len(), 2);

        let (key0, value0) = root_node.as_dict().unwrap().get(0).unwrap();
        assert_eq!(key0, b"a");
        assert_eq!(value0.node_type(), NodeType::Dict);
        assert_eq!(value0.as_dict().unwrap().len(), 2);

        let (key00, value00) = value0.as_dict().unwrap().get(0).unwrap();
        assert_eq!(key00, b"b");
        assert_eq!(value00.node_type(), NodeType::Int);
        assert_eq!(value00.as_int().unwrap().value().unwrap(), 1);

        let (key01, value01) = value0.as_dict().unwrap().get(1).unwrap();
        assert_eq!(key01, b"c");
        assert_eq!(value01.node_type(), NodeType::Str);
        assert_eq!(value01.as_string().unwrap().as_bytes(), b"abcd");

        let (key1, value1) = root_node.as_dict().unwrap().get(1).unwrap();
        assert_eq!(key1, b"d");
        assert_eq!(value1.node_type(), NodeType::Int);
        assert_eq!(value1.as_int().unwrap().value().unwrap(), 3);
    }

    #[test]
    fn test_list_size() {
        for x in 0..100 {
            let mut bencode_buf = "l".to_string();
            for y in 0..x {
                // bencode_buf += &format!("{}:{}", y, "X".repeat(y));
                // bencode_buf += &format!("i{}e", y);
                bencode_buf += &format!("li{}ee", y);
            }
            bencode_buf += "e";
            let bencode = bdecode(bencode_buf.as_bytes()).unwrap();
            let root_node = bencode.get_root();
            println!("{:?}", root_node);
            assert_eq!(root_node.as_list().unwrap().len(), x)
        }
    }
}
