//! Physical-quantity type system for Electronics Agent Kit (Entities ring, P9).
//!
//! Values carrying physical meaning are [`PhysicalQuantity`]s with a [`Unit`] and a
//! [`Tolerance`]. Units normalize to SI base units so equality and comparison are
//! dimensionally unambiguous; cross-dimension operations are type errors, never silent.
//! See `docs/engineering/units-and-quantities.md` and `docs/decisions/0007-*.md`.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A physical dimension. The SI base unit chosen per dimension is noted in the comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    Voltage,     // volt
    Current,     // ampere
    Power,       // watt
    Length,      // metre
    Temperature, // kelvin
    Frequency,   // hertz
    Time,        // second
    Resistance,  // ohm
    Capacitance, // farad
    Inductance,  // henry
    Dimensionless,
}

/// A concrete unit. Each converts to its dimension's SI base via the affine map
/// `si = value * scale + offset` (offset is non-zero only for Celsius).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Unit {
    Volt,
    Millivolt,
    Ampere,
    Milliampere,
    Watt,
    Milliwatt,
    Metre,
    Millimetre,
    Mil,
    Kelvin,
    DegreeCelsius,
    Hertz,
    Kilohertz,
    Megahertz,
    Second,
    Millisecond,
    Microsecond,
    Ohm,
    Kilohm,
    Farad,
    Microfarad,
    Nanofarad,
    Picofarad,
    Henry,
    Microhenry,
    Nanohenry,
    Unitless,
}

impl Unit {
    pub fn dimension(self) -> Dimension {
        use Unit::*;
        match self {
            Volt | Millivolt => Dimension::Voltage,
            Ampere | Milliampere => Dimension::Current,
            Watt | Milliwatt => Dimension::Power,
            Metre | Millimetre | Mil => Dimension::Length,
            Kelvin | DegreeCelsius => Dimension::Temperature,
            Hertz | Kilohertz | Megahertz => Dimension::Frequency,
            Second | Millisecond | Microsecond => Dimension::Time,
            Ohm | Kilohm => Dimension::Resistance,
            Farad | Microfarad | Nanofarad | Picofarad => Dimension::Capacitance,
            Henry | Microhenry | Nanohenry => Dimension::Inductance,
            Unitless => Dimension::Dimensionless,
        }
    }

    fn scale(self) -> f64 {
        use Unit::*;
        match self {
            Volt | Ampere | Watt | Metre | Kelvin | DegreeCelsius | Hertz | Second | Ohm
            | Farad | Henry | Unitless => 1.0,
            Millivolt | Milliampere | Milliwatt | Millimetre | Millisecond => 1e-3,
            Microsecond | Microfarad | Microhenry => 1e-6,
            Nanofarad | Nanohenry => 1e-9,
            Picofarad => 1e-12,
            Mil => 2.54e-5,
            Kilohertz | Kilohm => 1e3,
            Megahertz => 1e6,
        }
    }

    fn offset(self) -> f64 {
        match self {
            Unit::DegreeCelsius => 273.15,
            _ => 0.0,
        }
    }

    pub fn symbol(self) -> &'static str {
        use Unit::*;
        match self {
            Volt => "V",
            Millivolt => "mV",
            Ampere => "A",
            Milliampere => "mA",
            Watt => "W",
            Milliwatt => "mW",
            Metre => "m",
            Millimetre => "mm",
            Mil => "mil",
            Kelvin => "K",
            DegreeCelsius => "degC",
            Hertz => "Hz",
            Kilohertz => "kHz",
            Megahertz => "MHz",
            Second => "s",
            Millisecond => "ms",
            Microsecond => "us",
            Ohm => "ohm",
            Kilohm => "kohm",
            Farad => "F",
            Microfarad => "uF",
            Nanofarad => "nF",
            Picofarad => "pF",
            Henry => "H",
            Microhenry => "uH",
            Nanohenry => "nH",
            Unitless => "",
        }
    }
}

/// Tolerance on a [`PhysicalQuantity`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Tolerance {
    None,
    /// Relative fraction, e.g. `Relative(0.05)` == +/-5%.
    Relative(f64),
    /// Absolute bounds expressed in the quantity's own unit.
    Absolute {
        plus: f64,
        minus: f64,
    },
}

/// A typed physical value: magnitude in a [`Unit`], with optional [`Tolerance`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicalQuantity {
    pub magnitude: f64,
    pub unit: Unit,
    pub tolerance: Tolerance,
}

/// Error from a dimensionally- or numerically-invalid operation (P9 — no silent errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitError {
    DimensionMismatch {
        left: Dimension,
        right: Dimension,
    },
    /// A magnitude was not a finite number (e.g. NaN), so the quantities cannot be ordered
    /// or compared. Surfaced rather than silently treated as equal.
    NotComparable,
}

impl std::fmt::Display for UnitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitError::DimensionMismatch { left, right } => {
                write!(f, "dimension mismatch: {left:?} vs {right:?}")
            }
            UnitError::NotComparable => {
                write!(f, "physical quantity has a non-finite (NaN) magnitude")
            }
        }
    }
}
impl std::error::Error for UnitError {}

impl PhysicalQuantity {
    pub fn new(magnitude: f64, unit: Unit) -> Self {
        Self {
            magnitude,
            unit,
            tolerance: Tolerance::None,
        }
    }

    pub fn with_tolerance(magnitude: f64, unit: Unit, tolerance: Tolerance) -> Self {
        Self {
            magnitude,
            unit,
            tolerance,
        }
    }

    pub fn dimension(&self) -> Dimension {
        self.unit.dimension()
    }

    /// Magnitude expressed in the dimension's SI base unit.
    pub fn si_magnitude(&self) -> f64 {
        self.magnitude * self.unit.scale() + self.unit.offset()
    }

    /// Order two quantities of the same dimension; errors across dimensions (P9).
    pub fn try_compare(&self, other: &Self) -> Result<Ordering, UnitError> {
        if self.dimension() != other.dimension() {
            return Err(UnitError::DimensionMismatch {
                left: self.dimension(),
                right: other.dimension(),
            });
        }
        // A NaN magnitude has no ordering — surface it rather than silently calling it Equal.
        self.si_magnitude()
            .partial_cmp(&other.si_magnitude())
            .ok_or(UnitError::NotComparable)
    }

    /// True when both quantities denote the same physical value (within a relative epsilon).
    pub fn same_value(&self, other: &Self) -> Result<bool, UnitError> {
        if self.dimension() != other.dimension() {
            return Err(UnitError::DimensionMismatch {
                left: self.dimension(),
                right: other.dimension(),
            });
        }
        let (x, y) = (self.si_magnitude(), other.si_magnitude());
        if x.is_nan() || y.is_nan() {
            return Err(UnitError::NotComparable);
        }
        let scale = x.abs().max(y.abs()).max(1.0);
        Ok((x - y).abs() <= 1e-9 * scale)
    }
}

impl std::fmt::Display for PhysicalQuantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.magnitude, self.unit.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millivolts_normalize_to_volts() {
        let a = PhysicalQuantity::new(3300.0, Unit::Millivolt);
        let b = PhysicalQuantity::new(3.3, Unit::Volt);
        assert!((a.si_magnitude() - 3.3).abs() < 1e-9);
        assert!(a.same_value(&b).unwrap());
    }

    #[test]
    fn cross_dimension_compare_is_error() {
        let v = PhysicalQuantity::new(5.0, Unit::Volt);
        let i = PhysicalQuantity::new(5.0, Unit::Ampere);
        assert!(v.try_compare(&i).is_err());
        assert!(v.same_value(&i).is_err());
    }

    #[test]
    fn mil_and_celsius_convert() {
        let m = PhysicalQuantity::new(1.0, Unit::Mil);
        assert!((m.si_magnitude() - 2.54e-5).abs() < 1e-12);
        let c = PhysicalQuantity::new(25.0, Unit::DegreeCelsius);
        assert!((c.si_magnitude() - 298.15).abs() < 1e-9);
    }

    #[test]
    fn power_budget_ordering() {
        let limit = PhysicalQuantity::new(5.0, Unit::Watt);
        let measured = PhysicalQuantity::new(4200.0, Unit::Milliwatt);
        assert_eq!(measured.try_compare(&limit).unwrap(), Ordering::Less);
    }
}
