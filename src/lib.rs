mod parse_int;
mod stack_frame;
mod token;

use memchr::memchr;

use parse_int::{check_integer, decode_int, is_numeric};
use stack_frame::{StackFrame, StackFrameState};
use token::{Token, TokenType};

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
    tokens: Option<Vec<Token>>,
}

impl<'a> Node<'a> {
    pub fn print(&self) {
        match &self.tokens {
            Some(tokens) => {
                for token in tokens {
                    println!("{:?}", token);
                }
            }
            None => println!("None"),
        }
    }
}

pub fn bdecode<'a>(buf: &'a [u8]) -> Result<Node<'a>, BDecodeError> {
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
        tokens: Some(tokens),
    })
}
