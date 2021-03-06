// Core CBC Casper
// Copyright (C) 2018 - 2020  Coordination Technology Ltd.
// Authors: pZ4 <pz4@protonmail.ch>,
//          Lederstrumpf,
//          h4sh3d <h4sh3d@truelevel.io>
//          roflolilolmao <q@truelevel.ch>
//
// This file is part of Core CBC Casper.
//
// Core CBC Casper is free software: you can redistribute it and/or modify it under the terms
// of the GNU Affero General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// Core CBC Casper is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
// PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with the Core CBC
// Rust Library. If not, see <https://www.gnu.org/licenses/>.

extern crate core_cbc_casper;

use std::convert::From;

use core_cbc_casper::estimator::Estimator;
use core_cbc_casper::justification::LatestMessagesHonest;
use core_cbc_casper::message;
use core_cbc_casper::util::weight::{WeightUnit, Zero};
use core_cbc_casper::validator;

type Validator = u32;

pub type Message = message::Message<Value>;

#[derive(Debug, Hash, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, serde_derive::Serialize)]
pub enum Value {
    Zero = 0,
    One = 1,
    Two = 2,
}

impl<U: WeightUnit> From<((Value, U), (Value, U), (Value, U))> for Value {
    /// If equality between two or tree values exists, last value is
    /// prefered, then second value, and first value
    ///
    /// v1: w1 > w2,  w1 > w3
    /// v2: w2 >= w1, w2 > w3
    /// v3: w3 >= w1, w3 >= w1
    ///
    fn from(values: ((Value, U), (Value, U), (Value, U))) -> Self {
        let ((v1, w1), (v2, w2), (v3, w3)) = values;
        let mut max = v3;
        let mut weight = w3;
        if w2 > weight {
            max = v2;
            weight = w2;
        }
        if w1 > weight {
            max = v1;
        }
        max
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
    fn from(string: &'static str) -> Self {
        Error(string)
    }
}

impl Estimator for Value {
    type ValidatorName = Validator;
    type Error = Error;

    fn estimate<U: WeightUnit>(
        latest_messages: &LatestMessagesHonest<Value>,
        validators_weights: &validator::Weights<Validator, U>,
    ) -> Result<Self, Self::Error> {
        let res: Self = latest_messages
            .iter()
            .map(|message| {
                (
                    message.estimate(),
                    validators_weights.weight(message.sender()),
                )
            })
            .fold(
                (
                    (Value::Zero, <U as Zero<U>>::ZERO),
                    (Value::One, <U as Zero<U>>::ZERO),
                    (Value::Two, <U as Zero<U>>::ZERO),
                ),
                |acc, tuple| match tuple {
                    (Value::Zero, Ok(weight)) => (((acc.0).0, (acc.0).1 + weight), acc.1, acc.2),
                    (Value::One, Ok(weight)) => (acc.0, ((acc.1).0, (acc.1).1 + weight), acc.2),
                    (Value::Two, Ok(weight)) => (acc.0, acc.1, ((acc.2).0, (acc.2).1 + weight)),
                    _ => acc, // No weight for the given validator, do nothing
                },
            )
            .into();
        Ok(res)
    }
}

fn main() {
    use std::collections::HashSet;

    use core_cbc_casper::justification::{Justification, LatestMessages};

    let validators: Vec<u32> = (1..=4).collect();
    let weights = [0.6, 1.0, 2.0, 1.3];

    let validators_weights = validator::Weights::new(
        validators
            .iter()
            .cloned()
            .zip(weights.iter().cloned())
            .collect(),
    );

    let validator_state = validator::State::new(
        validators_weights,
        0.0,
        LatestMessages::empty(),
        1.0,
        HashSet::new(),
    );

    // 1: (1)  (2)
    // 2: (2)       (0)
    // 3: (0)  (0)       (0)
    // 4: (1)  (0)

    let message1 = Message::new(1, Justification::empty(), Value::One);
    let message2 = Message::new(2, Justification::empty(), Value::Two);
    let message3 = Message::new(3, Justification::empty(), Value::Zero);
    let message4 = Message::new(4, Justification::empty(), Value::One);
    let mut validator_state_clone = validator_state.clone();
    validator_state_clone.update(&[&message1, &message2]);
    let message5 = Message::from_validator_state(1, &validator_state_clone).unwrap();
    let mut validator_state_clone = validator_state.clone();
    validator_state_clone.update(&[&message3, &message4]);
    let message6 = Message::from_validator_state(3, &validator_state_clone).unwrap();
    let mut validator_state_clone = validator_state.clone();
    validator_state_clone.update(&[&message2, &message5, &message6]);
    let message7 = Message::from_validator_state(2, &validator_state_clone).unwrap();
    let mut validator_state_clone = validator_state.clone();
    validator_state_clone.update(&[&message7, &message6]);
    let message8 = Message::from_validator_state(3, &validator_state_clone).unwrap();
    let mut validator_state_clone = validator_state;
    validator_state_clone.update(&[&message4, &message6]);
    let message9 = Message::from_validator_state(4, &validator_state_clone).unwrap();

    assert_eq!(message5.estimate(), &Value::Two);
    assert_eq!(message6.estimate(), &Value::Zero);
    assert_eq!(message7.estimate(), &Value::Zero);
    assert_eq!(message8.estimate(), &Value::Zero);
    assert_eq!(message9.estimate(), &Value::Zero);
}
