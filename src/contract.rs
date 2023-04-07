#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{add_deposit, Asset, AssetType, Deposit, FeeConfig, Offer, FEE_CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:over-the-counter";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    FEE_CONFIG.save(
        deps.storage,
        &FeeConfig {
            fee_address: deps.api.addr_validate(&msg.fee_address)?,
            service_fee: msg.optional_service_fee,
        },
    )?;

    Ok(Response::new()
        .add_attribute("instantiate", "over-the-counter")
        .add_attribute("fee-address", msg.fee_address.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit { exchange, from } => execute::deposit(deps, info, exchange, from),
        _ => unimplemented!(),
    }
}

mod execute {
    use super::*;

    pub fn deposit(
        deps: DepsMut,
        info: MessageInfo,
        exchange: Asset,
        from: Option<String>,
    ) -> Result<Response, ContractError> {
        let from = if let Some(from) = from {
            Some(deps.api.addr_validate(&from)?)
        } else {
            None
        };

        let funds = info
            .funds
            .first()
            .cloned()
            .ok_or_else(|| ContractError::NoFundsWithDeposit {})?;

        let deposit = Deposit {
            deposit: Asset {
                denom: AssetType::Native(funds.denom.to_string()),
                amount: funds.amount,
            },
            offer: Offer {
                exchange: exchange.clone(),
                from,
            },
        };

        add_deposit(deps.storage, &info.sender, &deposit)?;

        Ok(Response::new()
            .add_attribute("execute", "deposit")
            .add_attribute("sender", info.sender.to_string())
            .add_attribute("deposit", info.funds[0].to_string())
            .add_attribute("exchange", exchange.to_string()))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg(test)]
mod tests {}
