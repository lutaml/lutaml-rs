//! Exact lexical values used by generated consumers. No floating-point bridge.
use crate::contracts::{
    ContractError, ContractResult, ErrorReason, NeutralValue, TypeBinding, TypeBindings,
};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::OnceLock;

fn failure(reason: ErrorReason, detail: impl Into<String>) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: None,
        attribute: None,
        format: None,
        rule: None,
        location: "/value".into(),
        klass: None,
        detail: Some(detail.into()),
    })
}

fn unsigned(input: &str) -> (bool, &str) {
    match input.as_bytes().first() {
        Some(b'-') => (true, &input[1..]),
        Some(b'+') => (false, &input[1..]),
        _ => (false, input),
    }
}
fn digits(part: &str) -> bool {
    part.bytes().all(|b| b.is_ascii_digit())
}

/// The one lexical rule for exact decimals, shared by `ExactDecimal`, the
/// loader's default/facet checks and generated types: an optional sign, then
/// digits with an optional fraction (`1`, `1.5`, `5.`, `.5`, `+1`; never `.`,
/// exponents or separators). This is the portable-exact-json-v1 plain-decimal
/// form recorded by the shared exact-vectors fixture; it is deliberately
/// narrower than Ruby's `BigDecimal()` and refuses the exponent spelling
/// (`"0.15e1"`) that `BigDecimal#to_json` emits.
pub fn is_exact_decimal(input: &str) -> bool {
    let (_, unsigned) = unsigned(input);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    !(whole.is_empty() && fraction.is_empty()) && digits(whole) && digits(fraction)
}

/// The one lexical rule for exact integers: an optional sign and one or more
/// digits (`7`, `-7`, `+7`; never `7.`, `.7` or exponents).
pub fn is_exact_integer(input: &str) -> bool {
    let (_, unsigned) = unsigned(input);
    !unsigned.is_empty() && digits(unsigned)
}

/// Read a plain decimal lexical form without converting through f64.
pub fn parse_decimal(input: &str) -> ContractResult<BigRational> {
    let (negative, unsigned) = unsigned(input);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if !is_exact_decimal(input) {
        return Err(failure(
            ErrorReason::InvalidDocument,
            "expected a plain decimal string (portable-exact-json-v1 refuses exponent forms such as Ruby BigDecimal#to_json \"0.15e1\")",
        ));
    }
    let digits = format!("{whole}{fraction}");
    let mut numerator = BigInt::from_str(&digits)
        .map_err(|_| failure(ErrorReason::InvalidDocument, "invalid decimal digits"))?;
    if negative {
        numerator = -numerator;
    }
    let scale = u32::try_from(fraction.len())
        .map_err(|_| failure(ErrorReason::InvalidDocument, "decimal scale is too large"))?;
    Ok(BigRational::new(numerator, BigInt::from(10u8).pow(scale)))
}

/// Render only terminating rationals. Refusing 1/3 is a contract, not rounding.
pub fn render_decimal(value: &BigRational) -> ContractResult<String> {
    let mut denominator = value.denom().clone();
    let mut twos = 0u32;
    let mut fives = 0u32;
    while (&denominator % 2u8).is_zero() {
        denominator /= 2u8;
        twos += 1;
    }
    while (&denominator % 5u8).is_zero() {
        denominator /= 5u8;
        fives += 1;
    }
    if denominator != BigInt::one() {
        return Err(failure(
            ErrorReason::NonterminatingRational,
            "denominator contains a factor other than 2 or 5",
        ));
    }
    let scale = twos.max(fives);
    let scaled =
        value.numer() * BigInt::from(2u8).pow(scale - twos) * BigInt::from(5u8).pow(scale - fives);
    let sign = if scaled.is_negative() { "-" } else { "" };
    let digits = scaled.abs().to_str_radix(10);
    if scale == 0 {
        return Ok(format!("{sign}{digits}"));
    }
    let scale = scale as usize;
    let padded = if digits.len() <= scale {
        format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits)
    } else {
        digits
    };
    let split = padded.len() - scale;
    Ok(format!("{sign}{}.{}", &padded[..split], &padded[split..]))
}

/// Keeps authored precision, including trailing zeros and signed zero.
///
/// Parsing validates the lexical form in linear time; the exact rational is
/// built on first use (decimal-to-bignum conversion is super-linear, and most
/// callers only carry the authored text).
#[derive(Clone, Debug)]
pub struct ExactDecimal {
    lexical: String,
    value: OnceLock<BigRational>,
}
impl PartialEq for ExactDecimal {
    fn eq(&self, other: &Self) -> bool {
        // The lexical form determines the value, so it is the whole identity.
        self.lexical == other.lexical
    }
}
impl Eq for ExactDecimal {}
impl ExactDecimal {
    pub fn parse(input: &str) -> ContractResult<Self> {
        if !is_exact_decimal(input) {
            return Err(failure(
                ErrorReason::InvalidDocument,
                "expected a plain decimal string (portable-exact-json-v1 refuses exponent forms such as Ruby BigDecimal#to_json \"0.15e1\")",
            ));
        }
        Ok(Self {
            lexical: input.into(),
            value: OnceLock::new(),
        })
    }
    pub fn as_str(&self) -> &str {
        &self.lexical
    }
    pub fn rational(&self) -> &BigRational {
        self.value.get_or_init(|| {
            parse_decimal(&self.lexical).expect("lexical form was validated at parse")
        })
    }
    pub fn from_rational(value: BigRational) -> ContractResult<Self> {
        Ok(Self {
            lexical: render_decimal(&value)?,
            value: OnceLock::from(value),
        })
    }
}
impl Serialize for ExactDecimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.lexical)
    }
}
impl<'de> Deserialize<'de> for ExactDecimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let lexical = String::deserialize(deserializer)?;
        Self::parse(&lexical).map_err(serde::de::Error::custom)
    }
}

/// Portable `float` wire text: Ruby's shortest round-trip digits with one
/// exponent spelling (lowercase `e`, no `+`, no leading zeros, always a
/// fraction digit). Fixed notation when -4 < decimal-point position <= 15, or
/// position 16 with 17 shortest digits (Ruby's `Float#to_s` rule).
/// `{:e}` yields the shortest round-trip digits; only the layout is ours.
/// The finite check is a precondition: callers refuse NaN/Infinity first.
pub fn float_wire(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let sci = format!("{:e}", value.abs());
    let (mantissa, exponent) = sci
        .split_once('e')
        .expect("LowerExp always has an exponent");
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let decpt = exponent
        .parse::<i32>()
        .expect("LowerExp exponent is an integer")
        + 1;
    // Exact ties: Rust's shortest digits round a tie up, Ruby's dtoa rounds it
    // to even. `{:.n}` rounds the exact value half-to-even at the same width.
    let places = digits.len() as i32 - decpt;
    if places > 0 {
        let exact = format!("{:.*}", places as usize, value.abs());
        let exact: String = exact
            .chars()
            .filter(|c| *c != '.')
            .skip_while(|c| *c == '0')
            .collect();
        if exact != digits && exact.len() == digits.len() && exact.parse::<f64>().is_ok() {
            let scaled = format!("{}e{}", exact, -places);
            if scaled.parse::<f64>() == Ok(value.abs()) {
                digits = exact;
            }
        }
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Ruby's Float#to_s: fixed for -4 < decpt <= 15, and also at decpt 16
    // when the shortest digits fill all 17 places.
    if decpt > -4 && (decpt <= 15 || (decpt == 16 && digits.len() == 17)) {
        if decpt <= 0 {
            format!(
                "{sign}0.{}{digits}",
                "0".repeat(decpt.unsigned_abs() as usize)
            )
        } else if decpt as usize >= digits.len() {
            format!(
                "{sign}{digits}{}.0",
                "0".repeat(decpt as usize - digits.len())
            )
        } else {
            let split = decpt as usize;
            format!("{sign}{}.{}", &digits[..split], &digits[split..])
        }
    } else {
        let fraction = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{fraction}e{}", &digits[..1], decpt - 1)
    }
}

fn float_shape(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (number, exponent) = match unsigned.split_once('e') {
        Some((number, exponent)) => (number, Some(exponent)),
        None => (unsigned, None),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let Some((whole, fraction)) = number.split_once('.') else {
        return false;
    };
    digits(whole)
        && digits(fraction)
        && exponent.is_none_or(|e| digits(e.strip_prefix('-').unwrap_or(e)))
}

/// Read a canonical float string: the shape matches, the value is finite and
/// re-rendering it reproduces the input byte for byte.
pub fn parse_float_wire(text: &str) -> ContractResult<f64> {
    if !float_shape(text) {
        return Err(failure(
            ErrorReason::InvalidDocument,
            "expected a canonical float string",
        ));
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() && float_wire(value) == text => Ok(value),
        _ => Err(failure(
            ErrorReason::InvalidDocument,
            "float string is not in canonical form",
        )),
    }
}

pub fn canonical_float(text: &str) -> bool {
    parse_float_wire(text).is_ok()
}

/// IEEE-754 binary64 with a string-only serde boundary (JSON numbers refused).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortableFloat(f64);
impl Eq for PortableFloat {}
impl PortableFloat {
    pub fn new(value: f64) -> ContractResult<Self> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(failure(
                ErrorReason::NonSerializableDefault,
                "a non-finite float has no wire form",
            ))
        }
    }
    pub fn parse(input: &str) -> ContractResult<Self> {
        parse_float_wire(input).map(Self)
    }
    pub fn get(&self) -> f64 {
        self.0
    }
    pub fn to_wire(self) -> String {
        float_wire(self.0)
    }
}
impl Serialize for PortableFloat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_wire())
    }
}
impl<'de> Deserialize<'de> for PortableFloat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Arbitrary-size integer with a string-only serde boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactInteger {
    lexical: String,
    value: BigInt,
}
impl ExactInteger {
    pub fn parse(input: &str) -> ContractResult<Self> {
        if !is_exact_integer(input) {
            return Err(failure(
                ErrorReason::InvalidDocument,
                "expected an integer decimal string",
            ));
        }
        let value = BigInt::from_str(input)
            .map_err(|_| failure(ErrorReason::InvalidDocument, "invalid integer digits"))?;
        Ok(Self {
            lexical: input.into(),
            value,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.lexical
    }
    pub fn as_bigint(&self) -> &BigInt {
        &self.value
    }
}
impl Serialize for ExactInteger {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.lexical)
    }
}
impl<'de> Deserialize<'de> for ExactInteger {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let lexical = String::deserialize(deserializer)?;
        Self::parse(&lexical).map_err(serde::de::Error::custom)
    }
}

pub fn require_binding<'a>(
    id: &str,
    bindings: &'a TypeBindings,
) -> ContractResult<&'a TypeBinding> {
    bindings
        .get(id)
        .filter(|binding| !binding.rust_type.trim().is_empty())
        .ok_or_else(|| {
            failure(
                ErrorReason::MissingTypeBinding,
                format!("missing value-type binding: {id}"),
            )
        })
}

/// Supported numeric facets are explicit; other facets cannot silently vanish.
pub fn validate_decimal_facets(
    value: &ExactDecimal,
    facets: &BTreeMap<String, NeutralValue>,
) -> ContractResult<()> {
    for (name, bound) in facets {
        let accepted = match name.as_str() {
            "min_inclusive" | "min_exclusive" | "max_inclusive" | "max_exclusive" => {
                let NeutralValue::String(text) = bound else {
                    return Err(failure(
                        ErrorReason::InvalidDocument,
                        "decimal facet bounds must be strings",
                    ));
                };
                let threshold = parse_decimal(text)?;
                match name.as_str() {
                    "min_inclusive" => *value.rational() >= threshold,
                    "min_exclusive" => *value.rational() > threshold,
                    "max_inclusive" => *value.rational() <= threshold,
                    _ => *value.rational() < threshold,
                }
            }
            "total_digits" | "fraction_digits" => {
                let limit = match bound {
                    NeutralValue::Unsigned(n) => *n,
                    NeutralValue::Signed(n) if *n >= 0 => *n as u64,
                    _ => {
                        return Err(failure(
                            ErrorReason::InvalidDocument,
                            "digit facet must be a nonnegative integer",
                        ))
                    }
                };
                let rendered = render_decimal(value.rational())?;
                let unsigned = rendered.trim_start_matches('-');
                let digits = if name == "fraction_digits" {
                    unsigned.split_once('.').map_or(0, |(_, part)| part.len())
                } else {
                    unsigned
                        .chars()
                        .filter(|c| *c != '.')
                        .collect::<String>()
                        .trim_start_matches('0')
                        .len()
                        .max(1)
                };
                digits as u64 <= limit
            }
            _ => {
                return Err(failure(
                    ErrorReason::UnsupportedType,
                    format!("unsupported decimal facet: {name}"),
                ))
            }
        };
        if !accepted {
            return Err(failure(
                ErrorReason::InvalidDocument,
                format!("decimal violates {name}"),
            ));
        }
    }
    Ok(())
}
