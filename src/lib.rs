mod parse_int;
mod stack_frame;
mod token;

use memchr::memchr;

use parse_int::{check_integer, decode_int, is_numeric};
use stack_frame::{StackFrame, StackFrameState};
use token::{Token, TokenType};

use std::cell::Cell;
use std::convert::TryInto;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BDecodeError {
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
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NodeType {
    Dict,
    List,
    Str,
    Int,
}

pub struct Node<'a> {
    buf: &'a [u8],
    /// if this is the root node, that owns all the tokens, they live in this
    /// vector. If this is a sub-node, this field is not used, instead the
    /// `root_tokens` reference points to the root node's token.
    tokens: Vec<Token>,
}

impl<'a> Node<'a> {
    pub fn get_root<'t>(&'t self) -> NodeChild<'a, 't> {
        NodeChild {
            buf: self.buf,
            root_tokens: &self.tokens,
            token_idx: 0,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeChild<'a, 't> {
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BencodeError {
    TypeError,
    IndexError,
}

impl<'a, 't> NodeChild<'a, 't> {
    fn create_child(&self, token_idx: usize) -> NodeChild<'a, 't> {
        NodeChild {
            buf: self.buf,
            root_tokens: self.root_tokens,
            token_idx: token_idx,
            cached_lookup: Cell::new(None),
            size: Cell::new(None),
        }
    }

    pub fn node_type(&self) -> NodeType {
        let token_type = self.root_tokens[self.token_idx].token_type();
        match token_type {
            TokenType::Dict => NodeType::Dict,
            TokenType::List => NodeType::List,
            TokenType::Int => NodeType::Int,
            TokenType::Str => NodeType::Str,
            _ => unreachable!(),
        }
    }

    /// Returns the item in the list at the given index. Returns an error if
    /// this node is not a list.
    pub fn list_at(&self, index: usize) -> Result<NodeChild<'a, 't>, BencodeError> {
        if self.node_type() != NodeType::List {
            return Err(BencodeError::TypeError);
        }

        let mut token = self.token_idx + 1;
        let mut item = 0;

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
                self.size.set(Some(item));
                return Err(BencodeError::IndexError);
            }
        }

        // There's no point in caching the first item
        if index > 0 {
            self.cached_lookup.set(Some((token, index)));
        }

        Ok(self.create_child(token))
    }

    /// Returns how many items there are in this list. Returns an error if
    /// this node is not a list.
    pub fn list_size(&self) -> Result<usize, BencodeError> {
        if self.node_type() != NodeType::List {
            return Err(BencodeError::TypeError);
        }

        // Maybe we have the size cached
        if let Some(size) = self.size.get() {
            return Ok(size);
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

        self.size.set(Some(size));
        Ok(size)
    }

    pub fn dict_at(&self, index: usize) -> Result<(&'a [u8], NodeChild<'a, 't>), BencodeError> {
        if self.node_type() != NodeType::Dict {
            return Err(BencodeError::TypeError);
        }

        let mut token = self.token_idx + 1;
        let mut item = 0;

        // do we have a lookup cached?
        if let Some((last_token, last_index)) = self.cached_lookup.get() {
            if last_index >= index {
                token = last_token;
                item = last_index;
            }
        }

        while item < index {
            debug_assert_eq!(self.root_tokens[token].token_type(), TokenType::Str);

            // skip the key
            token += self.root_tokens[token].next_item();
            if self.root_tokens[token].token_type() == TokenType::End {
                // index out of range
                self.size.set(Some(item));
                return Err(BencodeError::IndexError);
            }
            // skip the value
            token += self.root_tokens[token].next_item();
            if self.root_tokens[token].token_type() == TokenType::End {
                // index out of range
                self.size.set(Some(item));
                return Err(BencodeError::IndexError);
            }
            item += 1;
        }

        // There's no point in caching the first item
        if index > 0 {
            self.cached_lookup.set(Some((token, index)));
        }

        let key_node = self.create_child(token);
        let key = key_node.string_buf()?;

        let value_token = token + self.root_tokens[token].next_item();
        let value_node = self.create_child(value_token);

        Ok((key, value_node))
    }

    pub fn dict_find(&self, key: &[u8]) -> Result<Option<NodeChild<'a, 't>>, BencodeError> {
        if self.node_type() != NodeType::Dict {
            return Err(BencodeError::TypeError);
        }

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
                return Ok(Some(NodeChild {
                    buf: self.buf,
                    root_tokens: self.root_tokens,
                    token_idx: token,
                    cached_lookup: Cell::new(None),
                    size: Cell::new(None),
                }));
            }
            // skip key
            token += t.next_item();
            assert_ne!(self.root_tokens[token].token_type(), TokenType::End);
            // skip value
            token += self.root_tokens[token].next_item();
        }

        Ok(None)
    }

    pub fn dict_size(&self) -> Result<usize, BencodeError> {
        if self.node_type() != NodeType::Dict {
            return Err(BencodeError::TypeError);
        }

        // Maybe we have the size cached
        if let Some(size) = self.size.get() {
            return Ok(size);
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

        self.size.set(Some(size));
        Ok(size)
    }

    pub fn string_buf(&self) -> Result<&'a [u8], BencodeError> {
        if self.node_type() != NodeType::Str {
            return Err(BencodeError::TypeError);
        }
        let t = &self.root_tokens[self.token_idx];
        let t_off = t.offset();
        let t_off_start = t.start_offset();

        let t_next = &self.root_tokens[self.token_idx + 1];
        let t_next_off = t_next.offset();

        let size = t_next_off - t_off - t_off_start;

        Ok(&self.buf[(t_off + t_off_start)..(t_off + t_off_start + size)])
    }
}

pub fn bdecode<'a, 't>(buf: &'a [u8]) -> Result<Node<'a>, BDecodeError> {
    if buf.len() > Token::MAX_OFFSET {
        return Err(BDecodeError::LimitExceeded);
    }
    if buf.len() == 0 {
        return Err(BDecodeError::UnexpectedEof);
    }
    let mut sp: usize = 0;
    let mut stack: Vec<StackFrame> = Vec::new();
    let mut tokens: Vec<Token> = Vec::new();
    let mut off = 0;
    while off < buf.len() {
        let byte = buf[off];
        let current_frame = sp;

        // if we're currently parsing a dictionary, assert that
        // every other node is a string.
        if (current_frame > 0)
            && tokens[stack[current_frame - 1].token()].token_type() == TokenType::Dict
        {
            if stack[current_frame - 1].state() == StackFrameState::Key {
                // the current parent is a dict and we are parsing a key.
                // only allow a digit (for a string) or 'e' to terminate
                if !is_numeric(byte) && byte != b'e' {
                    return Err(BDecodeError::ExpectedDigit);
                }
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
                        return Err(BDecodeError::UnexpectedEof);
                    }
                };
                // +1 here to point to the first digit, rather than 'i'
                check_integer(&buf[(off + 1)..end_index]).map_err(|_| BDecodeError::Overflow)?;
                let new_token = Token::new(off, TokenType::Int, 1, 1)?;
                tokens.push(new_token);
                debug_assert_eq!(buf[end_index], b'e');
                off = end_index + 1;
            }
            b'e' => {
                // end of list or dict
                if sp == 0 {
                    return Err(BDecodeError::UnexpectedEof);
                }
                if sp > 0
                    && (tokens[stack[sp - 1].token()].token_type() == TokenType::Dict)
                    && stack[sp - 1].state() == StackFrameState::Value
                {
                    // this means we're parsing a dictionary and about to parse a
                    // value associated with a key. Instead, we got a termination
                    return Err(BDecodeError::ExpectedValue);
                }
                // insert end-of-sequence token
                let end_token = Token::new(off, TokenType::End, 1, 0)?;
                tokens.push(end_token);
                // and back-patch the start of this sequence with the offset
                // to the next token we'll insert
                let top = stack[sp - 1].token();
                // subtract the token's own index, since this is a relative
                // offset
                let next_item = tokens.len();
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
                        return Err(BDecodeError::ExpectedColon);
                    }
                };
                debug_assert_eq!(buf[colon_index], b':');
                let string_length: usize = decode_int(&buf[off..colon_index])
                    .map_err(|_| BDecodeError::Overflow)?
                    .try_into()
                    .map_err(|_| BDecodeError::Overflow)?;
                // FIXME: Is this needed in my code?
                off = colon_index + 1;
                if off >= buf.len() {
                    return Err(BDecodeError::UnexpectedEof);
                }
                // remaining buffer size
                let remaining = buf.len() - off;
                if string_length > remaining {
                    // The remaining buffer size is not big enough to fit a
                    // string that big.
                    return Err(BDecodeError::UnexpectedEof);
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

        if sp == 0 {
            // this terminates the top level node, we're done!
            break;
        }
    }

    // one final end token
    tokens.push(Token::new(off, TokenType::End, 0, 0)?);

    Ok(Node {
        buf,
        tokens: tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_at() {
        let node = bdecode(b"l4:spami42ee").unwrap();
        let root = node.get_root();
        let result = root.list_at(1).unwrap();
        println!("{:?}", result);
    }

    #[test]
    fn test_list_size() {
        let node = bdecode(b"l4:spami42ee").unwrap();
        let root = node.get_root();
        assert_eq!(root.list_size().unwrap(), 2);
    }
}
