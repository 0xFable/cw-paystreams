use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use cw_storage_plus::Map;


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub count: i32,
    pub owner: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
// A PaymentStream is a State Object which contains the details for a Payment Stream between two parties
// Parties in this case being recipient and sender addresses
// The Stream stores information which can be payer-defined such as the rate of payment per second.
pub struct PaymentStream {
    pub deposit: Uint128,
    pub rate_per_second: Uint128,
    pub remaining_balance: Uint128,
    pub stop_time: u64,
    pub start_time: u64,
    pub recipient: Addr,
    pub sender: Addr,
    pub token_addr: Addr,
    pub is_entity: bool,
}



pub const STATE: Item<State> = Item::new("state");
pub const STREAMS: Map<(&Addr, &Addr), PaymentStream> = Map::new("streams");
