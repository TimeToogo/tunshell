use std::fmt::Display;
use std::{cmp, ops};

/// Represents a wrapping u32 used for tracking the order of bytes
/// transmitted over the connection.
#[derive(Debug, PartialEq, Copy, Clone, Hash, Eq)]
pub(super) struct SequenceNumber(pub(super) u32);

/// Sequence numbers will wrap after exceeding 32-bit space
impl ops::Add<SequenceNumber> for SequenceNumber {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

/// We implement comparisons of sequence numbers based on the following logic
/// For two sequence numbers A and B, A is greater than B if their difference > 0 and < 2^31
/// A is less than B if their difference is > -2^31 and < 0
///
/// The reason for this is not to support sorting of sequence numbers but to determine
/// if one sequence number is more "advanced" than another, while considering the potential to wrap.
/// Hence the following cases:
///  - Seq(2) > Seq(1)
///  - Seq(0) > Seq(u32::MAX)
impl cmp::PartialOrd for SequenceNumber {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        if self.0 == other.0 {
            return Some(cmp::Ordering::Equal);
        }

        if self.0 > other.0 {
            let diff = self.0 - other.0;

            if diff < u32::MAX / 2 {
                return Some(cmp::Ordering::Greater);
            } else {
                return Some(cmp::Ordering::Less);
            }
        } else {
            let diff = other.0 - self.0;

            if diff < u32::MAX / 2 {
                return Some(cmp::Ordering::Less);
            } else {
                return Some(cmp::Ordering::Greater);
            }
        }
    }
}

impl Display for SequenceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Seq({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(SequenceNumber(1) + SequenceNumber(1), SequenceNumber(2));

        // Test wrapping
        assert_eq!(
            SequenceNumber(u32::MAX) + SequenceNumber(1),
            SequenceNumber(0)
        );
    }

    #[test]
    fn test_gt() {
        assert_eq!(SequenceNumber(2) > SequenceNumber(1), true);
        assert_eq!(SequenceNumber(1000) > SequenceNumber(500), true);
        assert_eq!(SequenceNumber(u32::MAX / 2) > SequenceNumber(1), true);
        assert_eq!(SequenceNumber(0) > SequenceNumber(u32::MAX), true);

        assert_eq!(SequenceNumber(1) > SequenceNumber(2), false);
        assert_eq!(SequenceNumber(500) > SequenceNumber(1000), false);
        assert_eq!(SequenceNumber(1) > SequenceNumber(u32::MAX / 2), false);
        assert_eq!(SequenceNumber(u32::MAX) > SequenceNumber(0), false);

        assert_eq!(SequenceNumber(u32::MAX / 2 + 1) > SequenceNumber(1), false);
        assert_eq!(SequenceNumber(1) > SequenceNumber(1), false);
    }

    #[test]
    fn test_gte() {
        assert_eq!(SequenceNumber(2) >= SequenceNumber(1), true);
        assert_eq!(SequenceNumber(1000) >= SequenceNumber(500), true);
        assert_eq!(SequenceNumber(u32::MAX / 2) >= SequenceNumber(1), true);
        assert_eq!(SequenceNumber(0) >= SequenceNumber(u32::MAX), true);
        assert_eq!(SequenceNumber(1) >= SequenceNumber(1), true);

        assert_eq!(SequenceNumber(1) >= SequenceNumber(2), false);
        assert_eq!(SequenceNumber(500) >= SequenceNumber(1000), false);
        assert_eq!(SequenceNumber(1) >= SequenceNumber(u32::MAX / 2), false);
        assert_eq!(SequenceNumber(u32::MAX) >= SequenceNumber(0), false);

        assert_eq!(SequenceNumber(u32::MAX / 2 + 1) >= SequenceNumber(1), false);
    }

    #[test]
    fn test_lt() {
        assert_eq!(SequenceNumber(1) < SequenceNumber(2), true);
        assert_eq!(SequenceNumber(500) < SequenceNumber(1000), true);
        assert_eq!(SequenceNumber(1) < SequenceNumber(u32::MAX / 2), true);
        assert_eq!(SequenceNumber(u32::MAX) < SequenceNumber(0), true);

        assert_eq!(SequenceNumber(2) < SequenceNumber(1), false);
        assert_eq!(SequenceNumber(1000) < SequenceNumber(500), false);
        assert_eq!(SequenceNumber(u32::MAX / 2) < SequenceNumber(1), false);
        assert_eq!(SequenceNumber(0) < SequenceNumber(u32::MAX), false);

        assert_eq!(SequenceNumber(1) < SequenceNumber(u32::MAX / 2 + 1), false);
        assert_eq!(SequenceNumber(1) < SequenceNumber(1), false);
    }

    #[test]
    fn test_lte() {
        assert_eq!(SequenceNumber(1) <= SequenceNumber(2), true);
        assert_eq!(SequenceNumber(500) <= SequenceNumber(1000), true);
        assert_eq!(SequenceNumber(1) <= SequenceNumber(u32::MAX / 2), true);
        assert_eq!(SequenceNumber(u32::MAX) <= SequenceNumber(0), true);
        assert_eq!(SequenceNumber(1) <= SequenceNumber(1), true);

        assert_eq!(SequenceNumber(2) <= SequenceNumber(1), false);
        assert_eq!(SequenceNumber(1000) <= SequenceNumber(500), false);
        assert_eq!(SequenceNumber(u32::MAX / 2) <= SequenceNumber(1), false);
        assert_eq!(SequenceNumber(0) <= SequenceNumber(u32::MAX), false);

        assert_eq!(SequenceNumber(1) <= SequenceNumber(u32::MAX / 2 + 1), false);
    }
}
