//! A parser conforming to standard postgreSQL to parse the precision and scale
//! from a decimal token obtained from the lalrpop lexer. This module
//! exists to resolve a cyclic dependency between proof-of-sql
//! and proof-of-sql-parser.
//!
//! A decimal must have a decimal point. The lexer does not route
//! whole integers to this contructor.
use bigdecimal::{num_bigint::BigInt, BigDecimal, ParseBigDecimalError, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

/// Errors related to the processing of decimal values in proof-of-sql
#[derive(Error, Debug, PartialEq)]
pub enum DecimalError {
    /// Represents an error encountered during the parsing of a decimal string.
    #[error(transparent)]
    ParseError(#[from] ParseBigDecimalError),
    /// Error occurs when this decimal cannot fit in a primitive.
    #[error("Value out of range for target type")]
    OutOfRange,
    /// Error occurs when this decimal cannot be losslessly cast into a primitive.
    #[error("Fractional part of decimal is non-zero")]
    LossyCast,
    /// Cannot cast this decimal to a big integer
    #[error("Conversion to integer failed")]
    ConversionFailure,
}

/// An intermediate placeholder for a decimal
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct IntermediateDecimal {
    value: BigDecimal,
}

impl IntermediateDecimal {
    /// Get the integer part of the fixed-point representation of this intermediate decimal.
    pub fn value(&self) -> BigDecimal {
        self.value.clone()
    }

    /// Get the precision of the fixed-point representation of this intermediate decimal.
    pub fn precision(&self) -> u8 {
        self.value.digits() as u8
    }

    /// Get the scale of the fixed-point representation of this intermediate decimal.
    pub fn scale(&self) -> i8 {
        self.value.fractional_digit_count() as i8
    }

    /// Attempts to convert the decimal to `BigInt` while adjusting it to the specified precision and scale.
    /// Returns an error if the conversion cannot be performed due to precision or scale constraints.
    pub fn try_into_bigint_with_precision_and_scale(
        &self,
        precision: u8,
        scale: i8,
    ) -> Result<BigInt, DecimalError> {
        let scaled_decimal = self.value.with_scale(scale.into());
        if scaled_decimal.digits() > precision.into() {
            return Err(DecimalError::LossyCast);
        }
        let (d, _) = scaled_decimal.into_bigint_and_exponent();
        Ok(d)
    }
}

impl fmt::Display for IntermediateDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl FromStr for IntermediateDecimal {
    type Err = DecimalError;

    fn from_str(decimal_string: &str) -> Result<Self, Self::Err> {
        BigDecimal::from_str(decimal_string)
            .map(|value| IntermediateDecimal {
                value: value.normalized(),
            })
            .map_err(DecimalError::ParseError)
    }
}

impl From<i128> for IntermediateDecimal {
    fn from(value: i128) -> Self {
        IntermediateDecimal {
            value: BigDecimal::from(value),
        }
    }
}

impl From<i64> for IntermediateDecimal {
    fn from(value: i64) -> Self {
        IntermediateDecimal {
            value: BigDecimal::from(value),
        }
    }
}

impl TryFrom<&str> for IntermediateDecimal {
    type Error = DecimalError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        IntermediateDecimal::from_str(s)
    }
}

impl TryFrom<String> for IntermediateDecimal {
    type Error = DecimalError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        IntermediateDecimal::from_str(&s)
    }
}

impl TryFrom<IntermediateDecimal> for i128 {
    type Error = DecimalError;

    fn try_from(decimal: IntermediateDecimal) -> Result<Self, Self::Error> {
        if !decimal.value.is_integer() {
            return Err(DecimalError::LossyCast);
        }

        match decimal.value.to_i128() {
            Some(value) if (i128::MIN..=i128::MAX).contains(&value) => Ok(value),
            _ => Err(DecimalError::OutOfRange),
        }
    }
}

impl TryFrom<IntermediateDecimal> for i64 {
    type Error = DecimalError;

    fn try_from(decimal: IntermediateDecimal) -> Result<Self, Self::Error> {
        if !decimal.value.is_integer() {
            return Err(DecimalError::LossyCast);
        }

        match decimal.value.to_i64() {
            Some(value) if (i64::MIN..=i64::MAX).contains(&value) => Ok(value),
            _ => Err(DecimalError::OutOfRange),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_decimal_simple() {
        let decimal = "123.45".parse();
        assert!(decimal.is_ok());
        let unwrapped_decimal: IntermediateDecimal = decimal.unwrap();
        assert_eq!(unwrapped_decimal.to_string(), "123.45");
        assert_eq!(unwrapped_decimal.precision(), 5);
        assert_eq!(unwrapped_decimal.scale(), 2);
    }

    #[test]
    fn test_valid_decimal_with_leading_and_trailing_zeros() {
        let decimal = "000123.45000".parse();
        assert!(decimal.is_ok());
        let unwrapped_decimal: IntermediateDecimal = decimal.unwrap();
        assert_eq!(unwrapped_decimal.to_string(), "123.45");
        assert_eq!(unwrapped_decimal.precision(), 5);
        assert_eq!(unwrapped_decimal.scale(), 2);
    }

    #[test]
    fn test_accessors() {
        let decimal: IntermediateDecimal = "123.456".parse().unwrap();
        assert_eq!(decimal.to_string(), "123.456");
        assert_eq!(decimal.precision(), 6);
        assert_eq!(decimal.scale(), 3);
    }

    #[test]
    fn test_conversion_to_i128() {
        let valid_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("170141183460469231731687303715884105727").unwrap(),
        };
        assert_eq!(
            i128::try_from(valid_decimal),
            Ok(170141183460469231731687303715884105727i128)
        );

        let valid_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("123.000").unwrap(),
        };
        assert_eq!(i128::try_from(valid_decimal), Ok(123));

        let overflow_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("170141183460469231731687303715884105728").unwrap(),
        };
        assert_eq!(
            i128::try_from(overflow_decimal),
            Err(DecimalError::OutOfRange)
        );

        let valid_decimal_negative = IntermediateDecimal {
            value: BigDecimal::from_str("-170141183460469231731687303715884105728").unwrap(),
        };
        assert_eq!(
            i128::try_from(valid_decimal_negative),
            Ok(-170141183460469231731687303715884105728i128)
        );

        let non_integer = IntermediateDecimal {
            value: BigDecimal::from_str("100.5").unwrap(),
        };
        assert_eq!(i128::try_from(non_integer), Err(DecimalError::LossyCast));
    }

    #[test]
    fn test_conversion_to_i64() {
        let valid_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("9223372036854775807").unwrap(),
        };
        assert_eq!(i64::try_from(valid_decimal), Ok(9223372036854775807i64));

        let valid_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("123.000").unwrap(),
        };
        assert_eq!(i64::try_from(valid_decimal), Ok(123));

        let overflow_decimal = IntermediateDecimal {
            value: BigDecimal::from_str("9223372036854775808").unwrap(),
        };
        assert_eq!(
            i64::try_from(overflow_decimal),
            Err(DecimalError::OutOfRange)
        );

        let valid_decimal_negative = IntermediateDecimal {
            value: BigDecimal::from_str("-9223372036854775808").unwrap(),
        };
        assert_eq!(
            i64::try_from(valid_decimal_negative),
            Ok(-9223372036854775808i64)
        );

        let non_integer = IntermediateDecimal {
            value: BigDecimal::from_str("100.5").unwrap(),
        };
        assert_eq!(i64::try_from(non_integer), Err(DecimalError::LossyCast));
    }
}
