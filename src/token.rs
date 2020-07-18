use std::fmt;

use super::BDecodeError;

const OFFSET_MASK: u64 = 0xFFFF_FFF8_0000_0000;
const NEXT_ITEM_MASK: u64 = 0x0000_0007_FFFF_FFC0;
const HEADER_MASK: u64 = 0x0000_0000_0000_0038;
const TYPE_MASK: u64 = 0x0000_0000_0000_0007;

const OFFSET_OFFSET: u64 = 35;
const NEXT_ITEM_OFFSET: u64 = 6;
const HEADER_OFFSET: u64 = 3;
const TYPE_OFFSET: u64 = 0;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TokenType {
    Dict = 1,
    List = 2,
    Str = 3,
    Int = 4,
    /// the node with type 'end' is a logical node, pointing to the end of
    /// the bencoded buffer.
    End = 5,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Token {
    inner: u64,
}

impl Token {
    pub const MAX_OFFSET: usize = (1 << 29) - 1;
    pub const MAX_NEXT_ITEM: usize = (1 << 29) - 1;
    pub const MAX_HEADER: usize = (1 << 3) - 1;

    pub fn new(
        offset: usize,
        token_type: TokenType,
        next_item: usize,
        header: usize,
    ) -> Result<Token, BDecodeError> {
        if (offset > Self::MAX_OFFSET)
            || (next_item > Self::MAX_NEXT_ITEM)
            || (header > Self::MAX_HEADER)
        {
            return Err(BDecodeError::LimitExceeded);
        }

        let inner = ((offset as u64) << OFFSET_OFFSET)
            | ((next_item as u64) << NEXT_ITEM_OFFSET)
            | ((header as u64) << HEADER_OFFSET)
            | ((token_type as u64) << TYPE_OFFSET);

        Ok(Token { inner })
    }

    #[inline(always)]
    pub fn offset(&self) -> usize {
        ((self.inner & OFFSET_MASK) >> OFFSET_OFFSET) as usize
    }

    /// if this node is a member of a list, 'next_item' is the number of nodes
    /// to jump forward in th node array to get to the next item in the list.
    /// if it's a key in a dictionary, it's the number of step forwards to get
    /// to its corresponding value. If it's a value in a dictionary, it's the
    /// number of steps to the next key, or to the end node.
    /// this is the _relative_ offset to the next node
    #[inline(always)]
    pub fn next_item(&self) -> usize {
        ((self.inner & NEXT_ITEM_MASK) >> NEXT_ITEM_OFFSET) as usize
    }

    #[inline(always)]
    pub fn set_next_item(&mut self, new_next_item: usize) -> Result<(), BDecodeError> {
        if new_next_item > Self::MAX_NEXT_ITEM {
            return Err(BDecodeError::LimitExceeded);
        }
        let inner_zeroed_ni = self.inner & (!NEXT_ITEM_MASK);
        self.inner = inner_zeroed_ni | ((new_next_item as u64) << NEXT_ITEM_OFFSET);
        Ok(())
    }

    /// this is the number of bytes to skip forward from the offset to get to the
    /// first character of the string, if this is a string. This field is not
    /// used for other types. Essentially this is the length of the length prefix
    /// and the colon. Since a string always has at least one character of length
    /// prefix and always a colon, those 2 characters are implied.
    #[inline(always)]
    pub fn header(&self) -> usize {
        ((self.inner & HEADER_MASK) >> HEADER_OFFSET) as usize
    }

    #[inline]
    pub fn token_type(&self) -> TokenType {
        let type_int = ((self.inner & TYPE_MASK) >> TYPE_OFFSET) as usize;
        match type_int {
            1 => TokenType::Dict,
            2 => TokenType::List,
            3 => TokenType::Str,
            4 => TokenType::Int,
            5 => TokenType::End,
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("offset", &self.offset())
            .field("next_item", &self.next_item())
            .field("header", &self.header())
            .field("token_type", &self.token_type())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_token_fields() {
        let mut tok = Token::new(42, TokenType::Dict, 11, 7).unwrap();
        assert_eq!(tok.offset(), 42);
        assert_eq!(tok.token_type(), TokenType::Dict);
        assert_eq!(tok.next_item(), 11);
        assert_eq!(tok.header(), 7);

        tok.set_next_item(29312).unwrap();
        // After setting next item, the rest of the fields should stay the
        // same.
        assert_eq!(tok.offset(), 42);
        assert_eq!(tok.token_type(), TokenType::Dict);
        assert_eq!(tok.next_item(), 29312);
        assert_eq!(tok.header(), 7);
    }

    #[test]
    fn test_token_size() {
        assert_eq!(mem::size_of::<Token>(), 8);
    }
}
