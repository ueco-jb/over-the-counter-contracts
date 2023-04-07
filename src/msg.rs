use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Decimal;

use crate::state::{Asset, Deposit, ID};

#[cw_serde]
pub struct InstantiateMsg {
    pub fee_address: String,
    pub optional_service_fee: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit native tokens with an offer
    Deposit {
        exchange: Asset,
        from: Option<String>,
    },
    /// Withdraw a deposit
    /// If no ID specified, all sender's deposits will be withdrawn
    Withdraw { id: Option<ID> },
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
    deposits: Vec<(ID, Deposit)>,
}

#[cw_serde]
pub struct DepositByIdResponse {
    deposit: Deposit,
}
