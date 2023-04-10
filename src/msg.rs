use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal};
use cw20::Cw20ReceiveMsg;

use crate::state::{Asset, Deposit, ID};

#[cw_serde]
pub struct InstantiateMsg {
    pub fee_address: String,
    pub optional_service_fee: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Receive CW20 message for deposit of exchange acceptance
    Receive(Cw20ReceiveMsg),
    /// Deposit native tokens with an offer
    Deposit {
        // What user expects in return
        exchange: Asset,
        // Accept offer only from this address
        from: Option<String>,
    },
    /// Withdraw a deposit
    /// If no ID specified, all sender's deposits will be withdrawn
    Withdraw {
        id: Option<ID>,
    },
    /// Accepts exchange offer of given ID, executing the transaction
    AcceptExchange {
        deposit_id: ID,
    },
}

#[cw_serde]
pub enum ReceiveCw20Msg {
    Deposit {
        // What user expects in return
        exchange: Asset,
        // Accept offer only from this address
        from: Option<String>,
    },
    /// Accepts exchange offer of given ID, executing the transaction
    AcceptExchange { deposit_id: ID },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Query all deposits from address
    #[returns(DepositsBySenderResponse)]
    DepositsBySender { address: String },
    /// Query one deposit using only its ID
    #[returns(DepositByIdResponse)]
    DepositById { id: ID },
}

#[cw_serde]
pub struct DepositsBySenderResponse {
    pub deposits: Vec<(ID, Deposit)>,
}

#[cw_serde]
pub struct DepositByIdResponse {
    pub sender: Addr,
    pub deposit: Deposit,
}
