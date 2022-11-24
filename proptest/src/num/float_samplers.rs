//-
// Copyright 2022 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Alternative uniform float samplers because the ones provided by the rand crate are prone
//! to overflow. The samplers work by uniformly selecting from a set of equally spaced values in
//! the interval and the included bounds. Selection is slightly biased towards the bounds.

pub(crate) use self::f32::F32U;
pub(crate) use self::f64::F64U;

macro_rules! float_sampler {
    ($typ: ident, $int_typ: ident, $wrapper: ident) => {
        pub mod $typ {
            use rand::prelude::*;
            use rand::distributions::uniform::{
                SampleBorrow, SampleUniform, Uniform, UniformSampler,
            };

            #[must_use]
            // Returns the previous float value. In other words the greatest value representable
            // as a float such that `next_down(a) < a`. `-0.` is treated as `0.`.
            fn next_down(a: $typ) -> $typ {
                debug_assert!(a.is_finite() && a > $typ::MIN, "`next_down` invalid input: {}", a);
                if a == (0.) {
                    -$typ::from_bits(1)
                } else if a < 0. {
                    $typ::from_bits(a.to_bits() + 1)
                } else {
                    $typ::from_bits(a.to_bits() - 1)
                }
            }

            #[must_use]
            // Returns the unit in last place using the definition by John Harrison.
            // This is the distance between `a` and the next closest float. Note that
            // `ulp(1) = $typ::EPSILON/2`.
            fn ulp(a: $typ) -> $typ {
                debug_assert!(a.is_finite() && a > $typ::MIN, "`ulp` invalid input: {}", a);
                a.abs() - next_down(a.abs())
            }

            #[derive(Copy, Clone, Debug)]
            pub(crate) struct $wrapper($typ);

            impl From<$typ> for $wrapper {
                fn from(x: $typ) -> Self {
                    $wrapper(x)
                }
            }
            impl From<$wrapper> for $typ {
                fn from(x: $wrapper) -> Self {
                    x.0
                }
            }

            #[derive(Clone, Copy, Debug)]
            pub(crate) struct FloatUniform {
                uniform: Uniform<$int_typ>,
                values: SampleValueCollection,
            }

            impl UniformSampler for FloatUniform {

                type X = $wrapper;

                fn new<B1, B2>(low: B1, high: B2) -> Self
                where
                    B1: SampleBorrow<Self::X> + Sized,
                    B2: SampleBorrow<Self::X> + Sized,
                {
                    let low = low.borrow().0;
                    let high = high.borrow().0;

                    let values = SampleValueCollection::new_inclusive(low, next_down(high));

                    FloatUniform {
                        uniform: Uniform::new(0, values.count),
                        values,
                    }
                }

                fn new_inclusive<B1, B2>(low: B1, high: B2) -> Self
                where
                    B1: SampleBorrow<Self::X> + Sized,
                    B2: SampleBorrow<Self::X> + Sized,
                {
                    let low = low.borrow().0;
                    let high = high.borrow().0;

                    let values = SampleValueCollection::new_inclusive(low, high);

                    FloatUniform {
                        uniform: Uniform::new(0, values.count),
                        values,
                    }
                }

                fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
                    $wrapper(self.values.get(self.uniform.sample(rng)))
                }
            }

            impl SampleUniform for $wrapper {
                type Sampler = FloatUniform;
            }

            #[derive(Clone, Copy, Debug)]
            struct SampleValueCollection {
                start: $typ,
                end: $typ,
                step: $typ,
                count: $int_typ,
            }

            // Values greater than MAX_PRECISE_INT may be rounded when converted to float.
            const MAX_PRECISE_INT: $int_typ =
                (2 as $int_typ).pow($typ::MANTISSA_DIGITS);

            // The collection of sample values that may be generated by UniformF32U.
            impl SampleValueCollection {
                fn new_inclusive(low: $typ, high: $typ) -> Self {
                    assert!(low.is_finite(), "low finite");
                    assert!(high.is_finite(), "high finite");
                    assert!(high - low >= 0., "invalid range");

                    let min_abs = $typ::min(low.abs(), high.abs());
                    let max_abs = $typ::max(low.abs(), high.abs());

                    let gap = ulp(max_abs);

                    let (start, end, step) = if low.abs() < high.abs() {
                        (high, low, -gap)
                    } else {
                        (low, high, gap)
                    };

                    let min_gaps = min_abs / gap;
                    let max_gaps = max_abs / gap;
                    debug_assert!(
                        max_gaps.floor() == max_gaps,
                        "max_gaps is an integer"
                    );

                    let count = if low.signum() == high.signum() {
                        max_gaps as $int_typ - min_gaps.floor() as $int_typ
                    } else {
                        max_gaps as $int_typ + min_gaps.ceil() as $int_typ
                    } + 1;
                    debug_assert!(count - 1 <= 2 * MAX_PRECISE_INT);

                    Self {
                        start,
                        end,
                        step,
                        count,
                    }
                }

                fn get(&self, index: $int_typ) -> $typ {
                    assert!(index < self.count, "index out of bounds");

                    if index == self.count - 1 {
                        return self.end;
                    }

                    // `index` might be greater that `MAX_PERCISE_INT` which means
                    // `index as $typ` could round to a different integer and
                    // `index as $typ + self.start` would have a rounding error.
                    // Fortunately, `index` will never be larger than `2 * MAX_PRECISE_INT`
                    // (as asserted above) so the expression below will be free of rounding.
                    ((index / 2) as $typ).mul_add(
                        2. * self.step,
                        (index % 2) as $typ * self.step + self.start,
                    )
                }
            }

            #[cfg(test)]
            mod test {

                use super::*;
                use crate::prelude::*;

                fn sort((left, right): ($typ, $typ)) -> ($typ, $typ) {
                    if left < right {
                        (left, right)
                    } else {
                        (right, left)
                    }
                }

                fn finite() -> impl Strategy<Value = $typ> {
                    prop::num::$typ::NEGATIVE
                    | prop::num::$typ::POSITIVE
                    | prop::num::$typ::NORMAL
                    | prop::num::$typ::SUBNORMAL
                    | prop::num::$typ::ZERO
                }

                fn bounds() -> impl Strategy<Value = ($typ, $typ)> {
                    (finite(), finite()).prop_map(sort)
                }

                #[test]
                fn range_test() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (-1., 10.);
                    let uniform = FloatUniform::new($wrapper(low), $wrapper(high));

                    let samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    for s in samples {
                        assert!(low <= s && s < high);
                    }
                }

                #[test]
                fn range_end_bound_test() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1., 1. + $typ::EPSILON);
                    let uniform = FloatUniform::new($wrapper(low), $wrapper(high));

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    assert!(samples.all(|x| x == 1.));
                }

                #[test]
                fn inclusive_range_test() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (-1., 10.);
                    let uniform = FloatUniform::new_inclusive($wrapper(low), $wrapper(high));

                    let samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    for s in samples {
                        assert!(low <= s && s <= high);
                    }
                }

                #[test]
                fn inclusive_range_end_bound_test() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1., 1. + $typ::EPSILON);
                    let uniform = FloatUniform::new_inclusive($wrapper(low), $wrapper(high));

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    assert!(samples.any(|x| x == 1. + $typ::EPSILON));
                }

                #[test]
                fn all_floats_in_range_are_possible_1() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (1. - $typ::EPSILON, 1. + $typ::EPSILON);
                    let uniform = FloatUniform::new_inclusive($wrapper(low), $wrapper(high));

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)));
                    assert!(samples.any(|x| x == 1. - $typ::EPSILON / 2.));
                }

                #[test]
                fn all_floats_in_range_are_possible_2() {
                    use crate::test_runner::{RngAlgorithm, TestRng};

                    let mut test_rng = TestRng::deterministic_rng(RngAlgorithm::default());
                    let (low, high) = (0., MAX_PRECISE_INT as $typ);
                    let uniform = FloatUniform::new_inclusive($wrapper(low), $wrapper(high));

                    let mut samples = (0..100)
                        .map(|_| $typ::from(uniform.sample(&mut test_rng)))
                        .map(|x| x.fract());

                    assert!(samples.any(|x| x != 0.));
                }

                #[test]
                // We treat [-0., 0.] as [0., 0.] since the distance between -0. and 0. is 0.
                fn zero_sample_values() {
                    let values = SampleValueCollection::new_inclusive(-0., 0.);
                    assert_eq!((values.count, values.get(0)), (1, 0.));
                }

                #[test]
                fn max_precise_int_plus_one_is_rounded_down() {
                    assert_eq!(((MAX_PRECISE_INT + 1) as $typ) as $int_typ, MAX_PRECISE_INT);
                }

                proptest! {
                    #[test]
                    fn next_down_less_than_float(val in finite()) {
                        prop_assume!(val > $typ::MIN);
                        prop_assert!(next_down(val) <  val);
                    }

                    #[test]
                    fn no_value_between_float_and_next_down(val in finite()) {
                        prop_assume!(val > $typ::MIN);
                        let prev = next_down(val);
                        let avg = prev / 2. + val / 2.;
                        prop_assert!(avg == prev || avg == val);
                    }

                    #[test]
                    fn values_less_than_or_equal_to_max_precise_int_are_not_rounded(i in 0..=MAX_PRECISE_INT) {
                        prop_assert_eq!((i as $typ) as $int_typ, i);
                    }

                    #[test]
                    fn single_value_interval(value: $typ) {
                        let values = SampleValueCollection::new_inclusive(value, value);
                        prop_assert_eq!((values.count, values.get(0)), (1, value));
                    }

                    #[test]
                    fn incl_low_and_high_are_start_and_end((low, high) in bounds()) {
                        let values = SampleValueCollection::new_inclusive(low, high);

                        let count = values.count;

                        let bounds = (values.get(0), values.get(count - 1));
                        prop_assert_eq!(sort(bounds), (low, high));
                    }

                    #[test]
                    fn values_excluding_end_are_equally_spaced(
                      (low, high) in bounds(), indices: [prop::sample::Index; 32]) {
                        let values = SampleValueCollection::new_inclusive(low, high);

                        let size = (values.count - 1) as usize;
                        prop_assume!(size > 0);

                        let all_equal = indices.iter()
                            .map(|i| i.index(size) as $int_typ)
                            .map(|i| values.get(i + 1) - values.get(i))
                            .all(|g| g == values.step);
                        prop_assert!(all_equal);
                    }

                    #[test]
                    fn end_gap_smaller_but_positive((low, high) in bounds()) {
                        let values = SampleValueCollection::new_inclusive(low, high);

                        let n = values.count;
                        prop_assume!(n > 1);

                        let gap = (values.get(n - 1) - values.get(n - 2)).abs();
                        prop_assert!(0. < gap && gap <= values.step.abs());
                    }
                }
            }
        }
    };
}

float_sampler!(f32, u32, F32U);
float_sampler!(f64, u64, F64U);
