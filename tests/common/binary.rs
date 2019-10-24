// Core CBC Rust Library
// Copyright (C) 2018  Coordination Technology Ltd.
// Authors: pZ4 <pz4@protonmail.ch>,
//          Lederstrumpf,
//          h4sh3d <h4sh3d@truelevel.io>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use casper::estimator::Estimator;
use casper::justification::LatestMsgsHonest;
use casper::util::weight::{WeightUnit, Zero};
use casper::validator;

type Validator = u32;

#[derive(Clone, Eq, PartialEq, Debug, Hash, serde_derive::Serialize)]
pub struct BoolWrapper(pub bool);

#[cfg(feature = "integration_test")]
impl BoolWrapper {
    pub fn new(estimate: bool) -> Self {
        BoolWrapper(estimate)
    }
}

#[cfg(feature = "integration_test")]
impl<V: validator::ValidatorName> From<V> for BoolWrapper {
    fn from(_validator: V) -> Self {
        BoolWrapper::new(bool::default())
    }
}

#[derive(Debug)]
pub struct Error(&'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

impl std::convert::From<&'static str> for Error {
    fn from(e: &'static str) -> Self {
        Error(e)
    }
}

impl Estimator for BoolWrapper {
    type V = Validator;
    type Error = Error;

    /// Weighted count of the votes contained in the latest messages.
    fn estimate<U: WeightUnit>(
        latest_msgs: &LatestMsgsHonest<BoolWrapper, Validator>,
        validators_weights: &validator::Weights<Validator, U>,
    ) -> Result<Self, Self::Error> {
        // loop over all the latest messages
        let (true_w, false_w) = latest_msgs.iter().fold(
            (<U as Zero<U>>::ZERO, <U as Zero<U>>::ZERO),
            |(true_w, false_w), msg| {
                // get the weight for the validator
                let validator_weight = validators_weights.weight(msg.sender()).unwrap_or(U::NAN);

                // add the weight to the right accumulator
                if msg.estimate().0 {
                    (true_w + validator_weight, false_w)
                } else {
                    (true_w, false_w + validator_weight)
                }
            },
        );

        Ok(BoolWrapper(true_w >= false_w))
    }
}
