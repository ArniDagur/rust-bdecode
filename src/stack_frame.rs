use std::convert::TryInto;
use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StackFrameState {
    Key = 0,
    Value = 1,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StackFrame {
    inner: u32,
}

impl StackFrame {
    const TOKEN_MASK: u32 = u32::MAX ^ 1;
    const STATE_MASK: u32 = 1;

    pub fn new(token: u32, state: StackFrameState) -> StackFrame {
        StackFrame {
            inner: (token << 1) | state as u32,
        }
    }

    #[inline(always)]
    pub fn token(&self) -> usize {
        let token_u32 = (self.inner & Self::TOKEN_MASK) >> 1;
        token_u32.try_into().unwrap()
    }

    #[inline(always)]
    pub fn state(&self) -> StackFrameState {
        if (self.inner & Self::STATE_MASK) == 0 {
            StackFrameState::Key
        } else {
            StackFrameState::Value
        }
    }

    #[inline(always)]
    pub fn toggle_state(&mut self) {
        self.inner = self.inner ^ 1;
    }
}

impl fmt::Debug for StackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackFrame")
            .field("token", &self.token())
            .field("state", &self.state())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::mem;

    #[test]
    fn test_stack_frame() {
        let mut frame = StackFrame::new(23, StackFrameState::Key);
        assert_eq!(frame.token(), 23);
        for n in 0..=10 {
            if n % 2 == 0 {
                assert_eq!(frame.state(), StackFrameState::Key);
            } else {
                assert_eq!(frame.state(), StackFrameState::Value);
            }
            frame.toggle_state();
        }
    }

    #[test]
    fn test_stack_frame_size() {
        assert_eq!(mem::size_of::<StackFrame>(), 4);
    }
}
