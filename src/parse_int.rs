use std::iter::Iterator;

use super::BDecodeError;

/// Check if the given byte represent a numeric digit
#[inline(always)]
pub fn is_numeric(byte: u8) -> bool {
    // Allowed digits in an integer are:
    // | Decimal | Char | Description |
    // +---------+------+-------------+
    // | 48      | 0    | zero        |
    // | 49      | 1    | one         |
    // | 50      | 2    | two         |
    // | 51      | 3    | three       |
    // | 52      | 4    | four        |
    // | 53      | 5    | five        |
    // | 54      | 6    | six         |
    // | 55      | 7    | seven       |
    // | 56      | 8    | eight       |
    // | 57      | 9    | nine        |
    (48 <= byte) && (byte <= 57)
}

/// Given a byte string representation of a Bencoded integer, without a leading
/// minus sign for negative numbers, check whether there are any leading zeroes.
///
/// # Examples:
/// | Integer | Numeric Part | Leading zero   |
/// +---------+--------------+----------------+
/// | -4      | 4            | no             |
/// | 4       | 4            | no             |
/// | -04     | 04           | yes            |
/// | 04      | 04           | yes            |
/// | 0       | 0            | no (exception) |
/// | 00      | 00           | yes            |
#[inline(always)]
fn contains_leading_zeroes(numeric_part: &[u8]) -> bool {
    (numeric_part.len() >= 2) && (numeric_part[0] == b'0')
}

/// finds the end of an integer and verifies that it looks valid this does
/// not detect all overflows, just the ones that are an order of magnitude
/// beyond. Exact overflow checking is done when the integer value is queried
/// from a bdecode_node.
#[inline(always)]
pub fn check_integer(bytes: &[u8]) -> Result<(), BDecodeError> {
    if bytes.len() == 0 {
        return Err(BDecodeError::UnexpectedEof);
    }
    let negative = bytes[0] == '-' as u8;
    if negative && bytes.len() == 1 {
        return Err(BDecodeError::ExpectedDigit);
    }
    let numeric_part = &bytes[(negative as usize)..];
    let looks_like_a_number = numeric_part.iter().all(|c| is_numeric(*c));
    if !looks_like_a_number {
        return Err(BDecodeError::ExpectedDigit);
    }
    if contains_leading_zeroes(numeric_part) {
        return Err(BDecodeError::LeadingZero);
    }
    Ok(())
}

#[inline(always)]
fn decode_int_no_sign(bytes: &[u8], negative: bool) -> Result<i64, BDecodeError> {
    let mut result: i64 = 0;
    for &byte in bytes {
        if !is_numeric(byte) {
            return Err(BDecodeError::ExpectedDigit);
        }
        // This substraction never underflows because of the check above.
        let digit = byte - 48;
        result = match result.checked_mul(10) {
            Some(result) => result,
            None => return Err(BDecodeError::Overflow),
        };
        if negative {
            result = match result.checked_sub(digit.into()) {
                Some(result) => result,
                None => return Err(BDecodeError::Overflow),
            };
        } else {
            result = match result.checked_add(digit.into()) {
                Some(result) => result,
                None => return Err(BDecodeError::Overflow),
            };
        }
    }
    return Ok(result);
}

#[inline(always)]
pub fn decode_int(bytes: &[u8]) -> Result<i64, BDecodeError> {
    let (negative, integer) = match bytes[0] {
        b'-' => (true, decode_int_no_sign(&bytes[1..], true)?),
        b'0'..=b'9' => (false, decode_int_no_sign(bytes, false)?),
        _ => return Err(BDecodeError::ExpectedDigit),
    };
    if negative && integer == 0 {
        return Err(BDecodeError::NegativeZero);
    }
    return Ok(integer);
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_invalid_cases {
        ($($x: expr),*) => {{
            $(assert!(check_integer($x).is_err());)*
        }}
    }

    #[inline]
    fn assert_roundtrip(number: i64, result: bool) {
        let int_string = number.to_string();
        let int_bytes = int_string.as_bytes();
        assert!(check_integer(int_bytes).is_ok());
        assert_eq!(decode_int(int_bytes).unwrap() == number, result);
    }

    #[test]
    fn test_negative_zero() {
        // Negative zero is not allowed
        let neg_zero = b"-0";
        assert_eq!(decode_int(neg_zero), Err(BDecodeError::NegativeZero));
        // But normal zero is allowed
        let zero = b"0";
        assert_eq!(decode_int(zero).unwrap(), 0);
    }

    #[test]
    fn test_leading_zero() {
        test_invalid_cases!(
            b"042",
            b"0013",
            b"01012",
            b"-09005",
            b"010010000",
            b"0000012230100012"
        );
    }

    #[test]
    fn test_biggest_possible_number() {
        assert_roundtrip(i64::MAX, true);
    }

    #[test]
    fn test_smallest_possible_number() {
        assert_roundtrip(i64::MIN, true);
    }

    #[test]
    fn test_lots_of_numbers() {
        for n in -100_000..=100_000 {
            // Creating a string out of the int and then decoding its bytes
            // should work.
            assert_roundtrip(n, true);

            // Do the same but add leading whitespace. This should fail.
            let int_string_2 = " ".to_owned() + &n.to_string();
            let int_bytes_2 = int_string_2.as_bytes();
            assert!(check_integer(int_bytes_2).is_err());

            // Do the same but add a leading zero. This should fail.
            let int_string_3 = "0".to_owned() + &n.to_string();
            let int_bytes_3 = int_string_3.as_bytes();
            assert!(check_integer(int_bytes_3).is_err());

            // Do the same but add a leading plus sign. This should fail.
            let int_string_4 = "+".to_owned() + &n.to_string();
            let int_bytes_4 = int_string_4.as_bytes();
            assert!(check_integer(int_bytes_4).is_err());
        }
    }

    #[test]
    fn test_contains_leading_zeroes() {
        const BAD_EXAMPLES: &[&[u8]] = &[b"04", b"00"];
        const GOOD_EXAMPLES: &[&[u8]] = &[b"4", b"0"];
        for &bad in BAD_EXAMPLES {
            assert!(contains_leading_zeroes(bad));
        }
        for &good in GOOD_EXAMPLES {
            assert!(!contains_leading_zeroes(good));
        }
    }
}
