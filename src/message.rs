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

use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use rayon::prelude::*;
use serde::Serialize;

use crate::estimator::Estimator;
use crate::justification::{Justification, LatestMessagesHonest};
use crate::util::hash::Hash;
use crate::util::id::Id;
use crate::util::weight::WeightUnit;
use crate::validator;

#[derive(Debug)]
pub enum Error<E: std::error::Error> {
    Estimator(E),
    NoNewMessage,
}

impl<E: std::error::Error> std::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Estimator(err) => std::fmt::Display::fmt(&err, f),
            Error::NoNewMessage => writeln!(f, "No message could be added to the state"),
        }
    }
}

impl<E: std::error::Error> std::error::Error for Error<E> {}

// Mathematical definition of a casper message with (value, validator, justification).
#[derive(Clone, Eq, PartialEq)]
struct ProtoMessage<E: Estimator> {
    estimate: E,
    sender: E::ValidatorName,
    justification: Justification<E>,
}

impl<E: Estimator> Id for ProtoMessage<E> {
    type ID = Hash;
}

impl<E: Estimator> Serialize for ProtoMessage<E> {
    fn serialize<T: serde::Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        use serde::ser::SerializeStruct;

        let mut message = serializer.serialize_struct("Message", 3)?;
        let justification: Vec<_> = self.justification.iter().map(Message::id).collect();
        message.serialize_field("sender", &self.sender)?;
        message.serialize_field("estimate", &self.estimate)?;
        message.serialize_field("justification", &justification)?;
        message.end()
    }
}

/// Concrete Casper message containing a value as [`Estimator`], a
/// validator as [`ValidatorName`], and a justification as [`Justification`].
///
/// # Example
///
/// Using the [`VoteCount`] type message type for brevity's sake.
///
/// ```
/// use core_cbc_casper::VoteCount;
/// use core_cbc_casper::justification::Justification;
///
/// // Creating a vote message
/// let vote_message = VoteCount::create_vote_message(0, true);
///
/// assert_eq!(vote_message.sender(), &(0 as u32));
/// assert_eq!(vote_message.justification(), &Justification::empty());
/// assert_eq!(vote_message.estimate(), &VoteCount { yes: 1, no: 0 });
/// ```
///
/// Message values must implement [`Estimator`] to be valid for a `message::Message` and to produce
/// estimates.
///
/// [`Estimator`]: ../estimator/trait.Estimator.html
/// [`ValidatorName`]: ../validator/trait.ValidatorName.html
/// [`Justification`]: ../justification/struct.Justification.html
/// [`VoteCount`]: ../struct.VoteCount.html
#[derive(Eq, Clone)]
pub struct Message<E: Estimator>(Arc<ProtoMessage<E>>, Hash);

impl<E: Estimator> Message<E> {
    pub fn sender(&self) -> &E::ValidatorName {
        &self.0.sender
    }

    pub fn estimate(&self) -> &E {
        &self.0.estimate
    }

    pub fn justification(&self) -> &Justification<E> {
        &self.0.justification
    }

    pub fn new(sender: E::ValidatorName, justification: Justification<E>, estimate: E) -> Self {
        let proto = ProtoMessage {
            sender,
            justification,
            estimate,
        };
        // Message is not mutable, id is computed only once at creation
        let id = proto.id();
        Message(Arc::new(proto), id)
    }

    /// Creates a message from newly received messages contained in
    /// [`validator_state`], which is used to compute the [`latest honest messages`].
    ///
    /// [`validator_state`]: ../validator/struct.State.html
    /// [`latest honest messages`]: ../justification/struct.LatestMessagesHonest.html
    pub fn from_validator_state<U: WeightUnit>(
        sender: E::ValidatorName,
        validator_state: &validator::State<E, U>,
    ) -> Result<Self, Error<E::Error>> {
        let latest_messages_honest = LatestMessagesHonest::from_latest_messages(
            validator_state.latests_messages(),
            validator_state.equivocators(),
        );

        if latest_messages_honest.is_empty() {
            Err(Error::NoNewMessage)
        } else {
            let justification = Justification::from(latest_messages_honest.clone());

            let estimate =
                latest_messages_honest.make_estimate(&validator_state.validators_weights());
            estimate
                .map(|estimate| Self::new(sender, justification, estimate))
                .map_err(Error::Estimator)
        }
    }

    /// Parses every messages accessible from `self` and `other` by iterating over messages'
    /// [`justifications`] and returns true if any of those messages is an equivocation with
    /// another one. This method can only be used to know that a random validator is
    /// equivocating but not which one.
    ///
    /// This method is currently broken as it does not always find equivocations that should be
    /// accessible from the given messages. It is not commutative. It compares messages with
    /// themselves.
    ///
    /// [`justifications`]: ../justification/struct.Justification.html
    pub fn equivocates_indirect(
        &self,
        other: &Self,
        mut equivocators: HashSet<E::ValidatorName>,
    ) -> (bool, HashSet<E::ValidatorName>) {
        let is_equivocation = self.equivocates(other);
        let init = if is_equivocation {
            equivocators.insert(self.sender().clone());
            (true, equivocators)
        } else {
            (false, equivocators)
        };
        self.justification().iter().fold(
            init,
            |(acc_has_equivocations, acc_equivocators), self_prime| {
                // Note the rotation between other and self, done because descending only on self,
                // thus other has to become self on the recursion to get its justification visited.
                let (has_equivocation, equivocators) =
                    other.equivocates_indirect(self_prime, acc_equivocators.clone());
                let acc_equivocators = acc_equivocators.union(&equivocators).cloned().collect();
                (acc_has_equivocations || has_equivocation, acc_equivocators)
            },
        )
    }

    /// Math definition of the equivocation.
    pub fn equivocates(&self, other: &Self) -> bool {
        self != other
            && self.sender() == other.sender()
            && !other.depends(self)
            && !self.depends(other)
    }

    /// Checks whether self depends on other or not. Returns true if other is somewhere in the
    /// [`justification`] of self. Then recursively checks the justifications of the messages in the
    /// [`justification`] of self.  This check is heavy and works well only with messages where the
    /// dependency is found on the surface, which is what it was designed for.
    ///
    /// [`justification`]: ../justification/struct.Justification.html
    pub fn depends(&self, other: &Self) -> bool {
        // Although the recursion ends supposedly only at genesis message, the trick is the
        // following: it short-circuits while descending on the dependency tree, if it finds a
        // dependent message. when dealing with honest validators, this would return true very
        // fast. all the new derived branches of the justification will be evaluated in parallel.
        // Say a message is justified by two other messages, then the two other messages will be
        // processed on different threads. This applies recursively, so if each of the two
        // messages have say three messages in their justifications, then each of the two threads
        // will spawn three new threads to process each of the messages.  Thus, highly
        // parallelizable. When it shortcuts because in one thread a dependency was found, the
        // function returns true and all the computation on the other threads will be cancelled.
        fn recurse<E: Estimator>(
            lhs: &Message<E>,
            rhs: &Message<E>,
            visited: Arc<RwLock<HashSet<Message<E>>>>,
        ) -> bool {
            let justification = lhs.justification();

            // Math definition of dependency
            justification.contains(rhs)
                || justification
                    .par_iter()
                    .filter(|lhs_prime| {
                        visited
                            .read()
                            .map(|v| !v.contains(lhs_prime))
                            .ok()
                            .unwrap_or(true)
                    })
                    .any(|lhs_prime| {
                        let visited_prime = visited.clone();
                        let _ = visited_prime
                            .write()
                            .map(|mut v| v.insert(lhs_prime.clone()))
                            .ok();
                        recurse(lhs_prime, rhs, visited_prime)
                    })
        }
        let visited = Arc::new(RwLock::new(HashSet::new()));
        recurse(self, other, visited)
    }
}

impl<E: Estimator> Id for Message<E> {
    type ID = Hash;

    // Redefine id to not recompute the hash every time
    fn id(&self) -> Self::ID {
        self.1
    }
}

impl<E: Estimator> Serialize for Message<E> {
    fn serialize<T: serde::Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        serde::Serialize::serialize(&self.0, serializer)
    }
}

impl<E: Estimator> std::hash::Hash for Message<E> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

impl<E: Estimator> PartialEq for Message<E> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.id() == other.id()
    }
}

impl<E: Estimator> Debug for Message<E> {
    // Note: format used for rendering illustrative gifs from generative tests; modify with care.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "M{:?}({:?})", self.sender(), self.estimate().clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::tests_common::vote_count::VoteCount;

    use std::collections::HashSet;
    use std::iter::FromIterator;

    use crate::justification::LatestMessages;
    use crate::validator;

    #[test]
    fn message_equality() {
        let validator_state = validator::State::new(
            validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        let v0 = &VoteCount::create_vote_message(0, false);
        let v1 = &VoteCount::create_vote_message(1, true);
        let v0_prime = &VoteCount::create_vote_message(0, true);
        let v0_duplicate = &VoteCount::create_vote_message(0, false);

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[v0]);
        let m0 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[v0]);
        let message1 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[v0]);
        let message2 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        let mut validator_state_clone = validator_state;
        validator_state_clone.update(&[v0, &m0]);
        let message3 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        assert_eq!(v0, v0_duplicate, "v0 and v0_duplicate should be equal");
        assert_ne!(v0, v0_prime, "v0 and v0_prime should NOT be equal");
        assert_ne!(v0, v1, "v0 and v1 should NOT be equal");
        assert_eq!(message1, message2, "messages should be equal");
        assert_ne!(
            message1, message3,
            "message1 should be different than message3"
        );
    }

    #[test]
    fn message_depends() {
        let validator_state = validator::State::new(
            validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        let v0 = &VoteCount::create_vote_message(0, false);
        let v0_prime = &VoteCount::create_vote_message(0, true);

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[v0]);
        let m0 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[&v0]);
        let m0_2 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        let mut validator_state_clone = validator_state;
        validator_state_clone.update(&[v0, &m0_2]);
        let m1 = Message::from_validator_state(0, &validator_state_clone).unwrap();

        assert!(
            !v0.depends(v0_prime),
            "v0 does NOT depend on v0_prime as they are equivocating"
        );
        assert!(
            !m0.depends(&m0),
            "m0 does NOT depend on itself directly, by our impl choice"
        );
        assert!(!m0.depends(v0_prime), "m0 depends on v0 directly");
        assert!(m0.depends(v0), "m0 depends on v0 directly");
        assert!(m1.depends(&m0), "m1 DOES depend on m0");
        assert!(!m0.depends(&m1), "but m0 does NOT depend on m1");
        assert!(m1.depends(v0), "m1 depends on v0 through m0");
    }

    #[test]
    fn message_equivocates() {
        let mut validator_state = validator::State::new(
            validator::Weights::new(vec![(0, 1.0), (1, 1.0)].into_iter().collect()),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        let v0 = &VoteCount::create_vote_message(0, false);
        let v0_prime = &VoteCount::create_vote_message(0, true);
        let v1 = &VoteCount::create_vote_message(1, true);

        validator_state.update(&[&v0]);
        let m0 = Message::from_validator_state(0, &validator_state).unwrap();

        assert!(!v0.equivocates(v0), "should be all good");
        assert!(!v1.equivocates(&m0), "should be all good");
        assert!(!m0.equivocates(v1), "should be all good");
        assert!(v0.equivocates(v0_prime), "should be a direct equivocation");
        assert!(
            m0.equivocates(v0_prime),
            "should be an indirect equivocation, equivocates to m0 through v0"
        );
    }

    #[test]
    fn message_equivocates_indirect_direct_equivocation() {
        let v0 = VoteCount::create_vote_message(0, false);
        let v0_prime = VoteCount::create_vote_message(0, true);

        assert!(v0.equivocates_indirect(&v0_prime, HashSet::new()).0);
    }

    #[test]
    fn message_equivocates_indirect_semi_direct() {
        let mut validator_state = validator::State::new(
            validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        // v0   v1
        //  |   |
        // m1   |
        //      m2
        //
        // validator 1 is equivocating

        let v0 = VoteCount::create_vote_message(0, false);
        let v1 = VoteCount::create_vote_message(1, true);

        validator_state.update(&[&v0]);
        let m1 = Message::from_validator_state(1, &validator_state).unwrap();

        validator_state.update(&[&v1]);
        let m2 = Message::from_validator_state(2, &validator_state).unwrap();

        assert!(m2.equivocates_indirect(&m1, HashSet::new()).0);

        // Cannot see future messages
        assert!(!m2.equivocates_indirect(&v0, HashSet::new()).0);
        assert!(!v0.equivocates_indirect(&v1, HashSet::new()).0);
    }

    #[test]
    fn message_equivocates_indirect_commutativity() {
        let mut validator_state = validator::State::new(
            validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        // v0   v1
        //  |   |
        // m1   |
        //      m2
        //
        // validator 1 is equivocating

        let v0 = VoteCount::create_vote_message(0, false);
        let v1 = VoteCount::create_vote_message(1, true);

        validator_state.update(&[&v0]);
        let m1 = Message::from_validator_state(1, &validator_state).unwrap();

        validator_state.update(&[&v1]);
        let m2 = Message::from_validator_state(2, &validator_state).unwrap();

        // Messages are tried for equivocation in the following order:
        // 1. for m1.equivocates_indirect(m2):
        //     1. m1 _|_ m2
        //     2. m2 _|_ v0
        //     3. v0 _|_ v0
        //     4. v0 _|_ v1
        //
        // 2. for m2.equivocates_indirect(m1):
        //     1. m2 _|_ m1
        //     2. m1 _|_ v0
        //     3. v0 _|_ v0
        //     4. m1 _|_ v1
        //     5. v1 _|_ v0
        //
        // We can see that:
        // 1. The method is not commutative;
        // 2. It does not try every combinations of messages;
        // 3. It compares v0 with itself in both instances.

        assert!(!m1.equivocates_indirect(&m2, HashSet::new()).0);
        assert!(m2.equivocates_indirect(&m1, HashSet::new()).0);
    }

    #[test]
    fn message_equivocates_indirect_total_indirection() {
        let mut validator_state = validator::State::new(
            validator::Weights::new(
                vec![(0, 1.0), (1, 1.0), (2, 1.0), (3, 1.0)]
                    .into_iter()
                    .collect(),
            ),
            0.0,
            LatestMessages::empty(),
            0.0,
            HashSet::new(),
        );

        // v0   v1
        //  |   |
        // m1   |
        //  |   m2
        // m3
        //
        // validator 1 is equivocating

        let v0 = VoteCount::create_vote_message(0, false);
        let v1 = VoteCount::create_vote_message(1, true);

        let mut validator_state_clone = validator_state.clone();
        validator_state_clone.update(&[&v0]);
        let m1 = Message::from_validator_state(1, &validator_state_clone).unwrap();

        validator_state_clone.update(&[&v1]);
        let m2 = Message::from_validator_state(2, &validator_state_clone).unwrap();

        validator_state.update(&[&m1]);
        let m3 = Message::from_validator_state(3, &validator_state).unwrap();

        // In this case, only 1 is equivocating. m1 and v1 are independant of each other. Neither
        // m2 or m3 are faulty messages but they are on different protocol branches created by
        // 1's equivocation.
        assert!(m2.equivocates_indirect(&m3, HashSet::new()).0);
    }

    #[test]
    fn from_validator_state() {
        let v0 = VoteCount::create_vote_message(0, false);
        let v1 = VoteCount::create_vote_message(1, false);
        let v2 = VoteCount::create_vote_message(2, true);

        let mut latest_messages = LatestMessages::empty();
        latest_messages.update(&v0);
        latest_messages.update(&v1);
        latest_messages.update(&v2);

        let res = Message::from_validator_state(
            0,
            &validator::State::new(
                validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
                0.0,
                latest_messages,
                0.0,
                HashSet::new(),
            ),
        )
        .expect("No errors expected");

        assert_eq!(*res.estimate(), VoteCount { yes: 1, no: 2 });
        assert_eq!(*res.sender(), 0);
        assert_eq!(
            HashSet::<&Message<VoteCount>>::from_iter(res.justification().iter()),
            HashSet::from_iter(vec![&v0, &v1, &v2]),
        );
    }

    #[test]
    fn from_validator_state_duplicates() {
        let v0 = VoteCount::create_vote_message(0, true);
        let v0_prime = VoteCount::create_vote_message(0, true);

        let mut latest_messages = LatestMessages::empty();
        latest_messages.update(&v0);
        latest_messages.update(&v0_prime);

        let res = Message::from_validator_state(
            0,
            &validator::State::new(
                validator::Weights::new(vec![(0, 1.0)].into_iter().collect()),
                0.0,
                latest_messages,
                0.0,
                HashSet::new(),
            ),
        )
        .expect("No errors expected");

        assert_eq!(*res.estimate(), VoteCount { yes: 1, no: 0 });
        assert_eq!(*res.sender(), 0);
        assert_eq!(
            HashSet::<&Message<VoteCount>>::from_iter(res.justification().iter()),
            HashSet::from_iter(vec![&v0]),
        );
    }

    #[test]
    fn from_validator_state_equivocator() {
        let v0 = VoteCount::create_vote_message(0, false);
        let v0_prime = VoteCount::create_vote_message(0, true);
        let v1 = VoteCount::create_vote_message(1, true);

        let mut latest_messages = LatestMessages::empty();
        latest_messages.update(&v0);
        latest_messages.update(&v0_prime);
        latest_messages.update(&v1);

        let res = Message::from_validator_state(
            2,
            &validator::State::new(
                validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
                0.0,
                latest_messages,
                4.0,
                HashSet::new(),
            ),
        )
        .expect("No errors expected");

        // No messages from the equivator in the justification. The
        // result is the same as from_validator_state_equivocator_at_threshhold
        // because from_validator_state uses the latest honest messages.
        assert_eq!(*res.sender(), 2);
        assert_eq!(*res.estimate(), VoteCount { yes: 1, no: 0 });
        assert_eq!(
            HashSet::<&Message<VoteCount>>::from_iter(res.justification().iter()),
            HashSet::from_iter(vec![&v1]),
        );
    }

    #[test]
    fn from_validator_state_equivocator_at_threshhold() {
        let v0 = VoteCount::create_vote_message(0, false);
        let v0_prime = VoteCount::create_vote_message(0, true);
        let v1 = VoteCount::create_vote_message(1, true);

        let mut latest_messages = LatestMessages::empty();
        latest_messages.update(&v0);
        latest_messages.update(&v0_prime);
        latest_messages.update(&v1);

        let res = Message::from_validator_state(
            2,
            &validator::State::new(
                validator::Weights::new(vec![(0, 1.0), (1, 1.0), (2, 1.0)].into_iter().collect()),
                0.0,
                latest_messages,
                0.0,
                HashSet::new(),
            ),
        )
        .expect("No errors expected");

        // No messages from the equivator in the justification.
        assert_eq!(*res.sender(), 2);
        assert_eq!(*res.estimate(), VoteCount { yes: 1, no: 0 });
        assert_eq!(
            HashSet::<&Message<VoteCount>>::from_iter(res.justification().iter()),
            HashSet::from_iter(vec![&v1]),
        );
    }

    #[test]
    fn from_validator_state_only_equivocations() {
        // With an equivocator and only his messages in the State,
        // from_validator_state returns an error.

        let v0 = VoteCount::create_vote_message(0, false);
        let v0_prime = VoteCount::create_vote_message(0, true);

        let mut latest_messages = LatestMessages::empty();
        latest_messages.update(&v0);
        latest_messages.update(&v0_prime);

        let res = Message::<VoteCount>::from_validator_state(
            0,
            &validator::State::new(
                validator::Weights::new(vec![(0, 1.0)].into_iter().collect()),
                0.0,
                latest_messages,
                0.0,
                HashSet::new(),
            ),
        );
        match res {
            Err(Error::NoNewMessage) => (),
            _ => panic!("Expected NoNewMessage"),
        }
    }
}
