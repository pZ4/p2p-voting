use std::collections::{BTreeSet, HashSet};
use traits::{Estimate, Data};
use message::{AbstractMsg, Message};
use justification::{Justification, Weights};
use senders_weight::{SendersWeight};
use std::convert::{From};
type Validator = u32;

/// a genesis block should be a block with estimate Block with prevblock =
/// None and data. data will be the unique identifier of this blockchain
#[derive(Clone, Default, Eq, PartialEq, PartialOrd, Ord, Debug, Hash)]
pub struct Block {
    prevblock: Option<Box<Block>>,
    data: Option<<Block as Data>::Data>, // TODO: lift the Option when we have real data structures.
}

pub type BlockMsg = Message<Block /*Estimate*/, Validator /*Sender*/>;

#[derive(Clone, Eq, Debug, Ord, PartialOrd, PartialEq, Hash)]
pub struct Tx;

impl Data for Block {
    type Data = BTreeSet<Tx>;
    fn is_valid(&self, _data: &Self::Data) -> bool {
        unimplemented!()
    }
}

impl<'z> From<&'z BlockMsg> for Block {
    fn from(msg: &BlockMsg) -> Self {
        msg.get_estimate().clone()
    }
}

impl Block {
    pub fn new(
        prevblock: Option<Box<Block>>,
        data: Option<<Block as Data>::Data>,
    ) -> Self {
        Self { prevblock, data }
    }
    fn get_prevblock(&self) -> Option<&Box<Block>> {
        self.prevblock.as_ref()
    }
    fn get_data(&self) -> &Option<<Block as Data>::Data> {
        &self.data
    }
    pub fn from_prevblock_msg(
        prevblock_msg: Option<BlockMsg>,
        data: Option<<Block as Data>::Data>,
    ) -> Self {
        let prevblock = prevblock_msg.map(|m| Box::new(Block::from(&m)));
        Self { prevblock, data }
    }

    fn is_member(&self, rhs: &Self) -> bool {
        println!("in is_member rhs : {:?}", rhs);
        println!("in is_member self: {:?}", self);
        self == rhs
            || rhs
                .get_prevblock()
                .map(|prevblock| {
                    println!("out is_member prevblock: {:?}", prevblock);
                    println!("out is_member self     : {:?}", self);
                    self.is_member(prevblock)
                })
                .unwrap_or(false)
    }
}

impl Estimate for Block {
    type M = BlockMsg;
    fn mk_estimate(
        latest_msgs: &Justification<Self::M>,
        finalized_msg: Option<&Self::M>,
        weights: &Weights<<<Self as Estimate>::M as AbstractMsg>::Sender>,
        data: Option<<Self as Data>::Data>,
    ) -> Self {
        match latest_msgs.len() {
            0 => panic!(
                "Needs at least one latest message to be able to pick one"
            ),
            1 => {
                let block = Self::from_prevblock_msg(
                    latest_msgs.iter().next().map(|msg| msg.clone()),
                    data,
                );
                // TODO: here u have to verify that the data is consistent with the block choice
                // is_valid_estimate
                block
            },
            _ => {
                let heaviest_msg = latest_msgs
                    .ghost(finalized_msg, weights.get_senders_weights());
                Self::from_prevblock_msg(heaviest_msg, data)
            },
        }
    }
}

#[test]
fn example_usage() {
    let (sender0, sender1, sender2, sender3) = (0, 1, 2, 3); // miner identities
    let (weight0, weight1, weight2, weight3) = (1.0, 1.0, 2.0, 1.0); // and their corresponding weights
    let senders_weights = SendersWeight::new(
        [
            (sender0, weight0),
            (sender1, weight1),
            (sender2, weight2),
            (sender3, weight3),
        ].iter()
            .cloned()
            .collect(),
    );
    let weights = Weights::new(
        senders_weights.clone(),
        0.0,            // state fault weight
        1.0,            // subjective fault weight threshold
        HashSet::new(), // equivocators
    );

    let estimate = Block {
        prevblock: None,
        data: None,
    };
    let justification = Justification::new();
    let genesis_block_msg =
        BlockMsg::new(sender0, justification, estimate.clone());
    assert_eq!(
        genesis_block_msg.get_estimate(),
        &estimate,
        "genesis block with None as prevblock"
    );

    let (m1, weights) = BlockMsg::from_msgs(
        sender1,
        vec![&genesis_block_msg],
        None, // finalized_msg, could be genesis_block_msg
        &weights,
        None, // data
    );

    let (m2, weights) = BlockMsg::from_msgs(
        sender2,
        vec![&genesis_block_msg],
        None,
        &weights,
        None,
    );

    let (m3, weights) =
        BlockMsg::from_msgs(sender3, vec![&m1, &m2], None, &weights, None);

    assert_eq!(
        m3.get_estimate(),
        &Block::new(Some(Box::new(Block::from(&m2))), None),
        "should build on top of m2 as sender2 has more weight"
    );

    // assert!(m1.is_member(&m1), "equal blocks");
    // assert!(!m1.is_member(&m2));
    // assert!(!m2.is_member(&m1));
    // assert!(!m1.is_member(&m2));
    // assert!(m2.is_member(&m3));
    // assert!(!m3.is_member(&m2));
    // assert!(!m3.is_member(&m1));



    // assert!(
    //     Block::from(&m1).is_member(&Block::from(&m1)),
    //     "equal blocks"
    // );
    // assert!(Block::from(&m2).is_member(&Block::from(&m3)));
    // assert!(!Block::from(&m3).is_member(&Block::from(&m2)));
    // assert!(!Block::from(&m3).is_member(&Block::from(&m1)));


    assert!(!Block::from(&m1).is_member(&Block::from(&m2)));
    // assert!(!Block::from(&m2).is_member(&Block::from(&m1)));
}

// #[test]
// fn example_equal_weight() {
//     let (sender0, sender1, sender2, sender3, sender4) = (0, 1, 2, 3, 4); // miner identities
//     let (weight0, weight1, weight2, weight3, weight4) =
//         (1.0, 1.0, 1.0, 1.0, 1.0); // and their corresponding weights
//     let senders_weights = SendersWeight::new(
//         [
//             (sender0, weight0),
//             (sender1, weight1),
//             (sender2, weight2),
//             (sender3, weight3),
//             (sender4, weight4),
//         ].iter()
//             .cloned()
//             .collect(),
//     );
//     let weights = Weights::new(
//         senders_weights.clone(),
//         0.0,            // state fault weight
//         1.0,            // subjective fault weight threshold
//         HashSet::new(), // equivocators
//     );

//     let estimate = Block {
//         prevblock: None,
//         data: None,
//     };
//     let justification = Justification::new();
//     let genesis_block_msg = BlockMsg::new(sender0, justification, estimate.clone());
//     assert_eq!(
//         genesis_block_msg.get_estimate(),
//         &estimate,
//         "genesis block with None as prevblock"
//     );

//     let (m1, weights) = BlockMsg::from_msgs(
//         sender1,
//         vec![&genesis_block_msg],
//         None, // finalized_msg, could be genesis_block_msg
//         &weights,
//         None, // data
//     );

//     let (m2, weights) =
//         BlockMsg::from_msgs(sender2, vec![&genesis_block_msg], None, &weights, None);
//     let (m3, weights) =
//         BlockMsg::from_msgs(sender3, vec![&genesis_block_msg], None, &weights, None);
//     let (b4, weights) =
//         BlockMsg::from_msgs(sender4, vec![&genesis_block_msg], None, &weights, None);
//     let (b5, weights) =
//         BlockMsg::from_msgs(sender1, vec![&m1, &m2], None, &weights, None);
//     let (b6, weights) =
//         BlockMsg::from_msgs(sender2, vec![&m2], None, &weights, None);
//     let (b7, weights) =
//         BlockMsg::from_msgs(sender3, vec![&m3], None, &weights, None);
//     let (b8, weights) =
//         BlockMsg::from_msgs(sender4, vec![&m3, &b4], None, &weights, None);
//     let (b9, weights) =
//         BlockMsg::from_msgs(sender1, vec![&b5], None, &weights, None);
//     let (b10, weights) =
//         BlockMsg::from_msgs(sender2, vec![&b5], None, &weights, None);
//     let (b11, weights) =
//         BlockMsg::from_msgs(sender3, vec![&b8], None, &weights, None);
//     let (b12, weights) =
//         BlockMsg::from_msgs(sender4, vec![&b8], None, &weights, None);

//     assert_eq!(
//         b5.get_estimate(),
//         &Block::new(Some(m2.clone()), None),
//         "should build on top of m2"
//     );
// }
