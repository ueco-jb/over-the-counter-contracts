use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("No funds provided during deposit")]
    NoFundsWithDeposit {},

    #[error("No native token provided for exchange")]
    NoNativeForExchange {},

    #[error("Offer expected tokens: {expected}, user provided tokens: {received}")]
    ExchangeIncorrectDenom { expected: String, received: String },

    #[error("Incorrect amount has been provided, exchange expected {expected_amount} while user provided {provided_amount}")]
    ExchangeIncorrectAmount {
        expected_amount: Uint128,
        provided_amount: Uint128,
    },
}
