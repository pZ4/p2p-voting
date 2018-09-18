use traits::{Estimate, Data, Zero};
use message::{CasperMsg, Message};
use justification::{LatestMsgs};
use senders_weight::{SendersWeight};
use weight_unit::{WeightUnit};
use std::collections::HashSet;
use std::iter::FromIterator;
type Validator = u32;

pub type IntegerMsg = Message<u32 /*Estimate*/, Validator /*Sender*/>;

#[derive(Clone, Eq, Debug, Ord, PartialOrd, PartialEq, Hash)]
pub struct Tx;

impl Data for u32 {
    type Data = Self;
    fn is_valid(_data: &Self::Data) -> bool {
        true // FIXME
    }
}

impl Estimate for u32 {
    type M = IntegerMsg;
    fn mk_estimate(
        latest_msgs: &LatestMsgs<Self::M>,
        _finalized_msg: Option<&Self::M>,
        senders_weights: &SendersWeight<
            <<Self as Estimate>::M as CasperMsg>::Sender,
        >,
        // in fact i could put the whole mempool inside of this proto_block and
        // search for a reasonable set of txs in this function that does not
        // conflict with the past blocks
        _proto_block: Option<<Self as Data>::Data>,
    ) -> Self {
        let mut msgs_sorted_by_estimate = Vec::from_iter(latest_msgs.iter().fold(
            HashSet::new(),
            |latest, (_, latest_from_validator)| {
                latest.union(&latest_from_validator).cloned().collect()
            },
        ));
        msgs_sorted_by_estimate.sort_unstable_by(|a, b| {
            senders_weights
                .get_weight(a.get_sender())
                .unwrap_or(WeightUnit::ZERO)
                .partial_cmp(
                    &senders_weights
                        .get_weight(b.get_sender())
                        .unwrap_or(WeightUnit::ZERO),
                )
                .unwrap_or(::std::cmp::Ordering::Greater)
        });
        let total_weight =
            msgs_sorted_by_estimate.iter().fold(0.0, |acc, x| {
                acc + senders_weights
                    .get_weight(x.get_sender())
                    .unwrap_or(WeightUnit::ZERO)
            });
        let mut running_weight = 0.0;
        let mut current_msg = msgs_sorted_by_estimate.iter();
        while running_weight / total_weight < 0.5 {
            running_weight += senders_weights
                .get_weight(current_msg.next().unwrap().get_sender())
                .unwrap_or(WeightUnit::ZERO);
        }
        *current_msg.next().unwrap().get_estimate()
    }
}
