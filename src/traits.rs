use std::hash::{Hash};
use std::fmt::{Debug};

use message::{CasperMsg};
use justification::{LatestMsgsHonest};
use senders_weight::{SendersWeight};

pub trait Estimate: Hash + Clone + Ord + Send + Sync + Debug + Data {
    type M: CasperMsg<Estimate = Self>;
    fn mk_estimate(
        latest_msgs: &LatestMsgsHonest<Self::M>,
        finalized_msg: Option<&Self::M>,
        senders_weights: &SendersWeight<<<Self as Estimate>::M as CasperMsg>::Sender>,
        external_data: Option<<Self as Data>::Data>,
    ) -> Self;
}

pub trait Data {
    type Data;
    fn is_valid(&Self::Data) -> bool;
}

pub trait Sender: Hash + Clone + Ord + Eq + Send + Sync + Debug {}

pub trait Zero<T: PartialEq> {
    const ZERO: T;
    fn is_zero(val: &T) -> bool {
        val == &Self::ZERO
    }
}

/// Define how to serialize an arbitrary structure into as stream of bytes.
/// The serialization can be performed with any standard or non-standard formats
/// but the **serialization MUST ensure that only one representation is valid.**
pub trait Serialize {
    /// Serialize data into a byte stream.
    fn serialize(&self) -> Vec<u8>;
}

/// Define how to deserialize an arbitrary byte stream. **If the byte stream is not
/// the unique valid representation of the structure, deserialization MUST fail.**
///
/// ## Serialization malleability
///
/// If a structure can be represented with multiple valid byte streams, then the
/// content identifier is not anymore unique. The implementation MUST ensure only
/// one unique valid representation. To ensure non-malleability we allow deserialization
/// to fail.
pub trait Deserialize where Self: Sized {
    /// Deserialize a byte stream and return the result. MUST FAIL if the byte stream
    /// is NOT the unique valid representation.
    fn deserialize(bin: &[u8]) -> Result<Self, ()>;
}

/// Define a content able to identifie its content with an ID. The structure must
/// be serializable with no malleability to ensure unique valid identifiers for
/// every unique valid content.
pub trait Id: Serialize {
    /// Define the type of the ID generated by `getid`.
    type ID;

    /// Define the hashing algorithm used to get content ID based on the serialization
    /// provided by the default `getid` method.
    fn hash(data: &[u8]) -> Self::ID;

    /// The default method for getting the content ID is based on the serialization of
    /// the content. This method can be overriden by other mechanisms such as random
    /// or counter IDs.
    fn getid(&self) -> Self::ID {
        let ser = self.serialize();
        Self::hash(&ser[..])
    }
}

