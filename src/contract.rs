#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdResult, WasmMsg,
};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::msg::{DepositByIdResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    add_deposit, Asset, AssetType, Deposit, FeeConfig, Offer, DEPOSITS, FEE_CONFIG, ID,
};

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
        .add_attribute("fee-address", msg.fee_address))
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
        ExecuteMsg::Withdraw { id } => execute::withdraw(deps, info.sender, id),
        ExecuteMsg::AcceptExchange { deposit_id } => {
            execute::accept_exchange(deps, info, deposit_id)
        }
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
            .ok_or(ContractError::NoFundsWithDeposit {})?;

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

    pub fn withdraw(
        deps: DepsMut,
        sender: Addr,
        deposit_id: Option<ID>,
    ) -> Result<Response, ContractError> {
        let keys_to_remove = if let Some(id) = deposit_id {
            // If ID is provided, remove only the entry with the provided address and ID
            let deposit = DEPOSITS.load(deps.storage, (&sender, id))?.deposit;
            vec![((&sender, id), deposit)]
        } else {
            // If ID is not provided, remove all entries with the provided address prefix
            DEPOSITS
                .prefix(&sender)
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| {
                    let (id, deposit) = item?;
                    Ok(((&sender, id), deposit.deposit))
                })
                .collect::<StdResult<Vec<((&Addr, ID), Asset)>>>()?
        };

        let mut msgs = vec![];
        for (key, deposit) in keys_to_remove {
            DEPOSITS.remove(deps.storage, key);
            let msg: CosmosMsg = match deposit.denom {
                AssetType::Native(denom) => BankMsg::Send {
                    to_address: sender.to_string(),
                    amount: coins(deposit.amount.u128(), denom),
                }
                .into(),
                AssetType::Cw20(denom) => WasmMsg::Execute {
                    contract_addr: denom.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: sender.to_string(),
                        amount: deposit.amount,
                    })?,
                    funds: vec![],
                }
                .into(),
            };
            msgs.push(msg);
        }

        Ok(Response::new()
            .add_messages(msgs)
            .add_attribute("action", "withdraw")
            .add_attribute("sender", sender.to_string()))
    }

    pub fn accept_exchange(
        deps: DepsMut,
        info: MessageInfo,
        deposit_id: ID,
    ) -> Result<Response, ContractError> {
        let DepositByIdResponse {
            sender: deposit_sender,
            deposit,
        } = query::deposit_by_id(deps.as_ref(), deposit_id)?;

        let funds = info
            .funds
            .first()
            .cloned()
            .ok_or(ContractError::NoNativeForExchange {})?;

        let exchange_messages = match deposit.offer.exchange.denom.clone() {
            AssetType::Native(denom) => {
                if funds.denom == denom {
                    if funds.amount == deposit.offer.exchange.amount {
                        // Create two messages
                        // First sends newly sent native funds to the depositor,
                        // second sends original deposit to user that accepted the exchange
                        create_exchange_messages(
                            &deposit_sender,
                            &Asset::new_native(funds.amount.into(), &funds.denom),
                            &info.sender,
                            &deposit.deposit,
                        )?
                    } else {
                        // User sent incorrect amount of native tokens to accept the exchange
                        return Err(ContractError::ExchangeIncorrectAmount {
                            expected_amount: deposit.offer.exchange.amount,
                            provided_amount: funds.amount,
                        });
                    }
                } else {
                    // User sent incorrect native token to the exchange
                    return Err(ContractError::ExchangeIncorrectNative {
                        expected: denom,
                        received: funds.denom,
                    });
                }
            }
            // User send native tokens to accept an exchange which expected CW20 token
            AssetType::Cw20(expected) => {
                return Err(ContractError::NativeTokenInsteadOfCw20 {
                    expected,
                    received: funds.denom,
                });
            }
        };

        Ok(Response::new()
            .add_messages(exchange_messages)
            .add_attribute("exchange", "completed")
            .add_attribute("deposit-sender", deposit_sender.to_string())
            .add_attribute("original-deposit", deposit.deposit.to_string())
            .add_attribute("expected", deposit.offer.exchange.to_string())
            .add_attribute("accepted-by", info.sender.to_string()))
    }

    pub fn create_exchange_messages(
        first_party: &Addr,
        first_asset: &Asset,
        second_party: &Addr,
        second_asset: &Asset,
    ) -> StdResult<Vec<CosmosMsg>> {
        let first_message: CosmosMsg = match first_asset.denom.clone() {
            AssetType::Native(denom) => BankMsg::Send {
                to_address: first_party.to_string(),
                amount: coins(first_asset.amount.u128(), denom),
            }
            .into(),
            AssetType::Cw20(denom) => WasmMsg::Execute {
                contract_addr: denom,
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: first_party.to_string(),
                    amount: first_asset.amount,
                })?,
                funds: vec![],
            }
            .into(),
        };
        let second_message: CosmosMsg = match second_asset.denom.clone() {
            AssetType::Native(denom) => BankMsg::Send {
                to_address: second_party.to_string(),
                amount: coins(second_asset.amount.u128(), denom),
            }
            .into(),
            AssetType::Cw20(denom) => WasmMsg::Execute {
                contract_addr: denom,
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: second_party.to_string(),
                    amount: second_asset.amount,
                })?,
                funds: vec![],
            }
            .into(),
        };
        Ok(vec![first_message, second_message])
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::DepositsBySender { address } => {
            to_binary(&query::deposits_by_sender(deps, address)?)
        }
        QueryMsg::DepositById { id } => to_binary(&query::deposit_by_id(deps, id)?),
    }
}

mod query {
    use cosmwasm_std::StdError;

    use crate::msg::{DepositByIdResponse, DepositsBySenderResponse};
    use crate::state::get_deposits;

    use super::*;

    pub fn deposits_by_sender(deps: Deps, address: String) -> StdResult<DepositsBySenderResponse> {
        let address = deps.api.addr_validate(&address)?;
        Ok(DepositsBySenderResponse {
            deposits: get_deposits(deps.storage, &address)?,
        })
    }

    pub fn deposit_by_id(deps: Deps, search_id: ID) -> StdResult<DepositByIdResponse> {
        let deposit = DEPOSITS
            .range(deps.storage, None, None, Order::Ascending)
            .filter_map(|item| match item {
                Ok(((sender, id), deposit)) => {
                    if id == search_id {
                        Some(Ok((sender, deposit)))
                    } else {
                        None
                    }
                }
                Err(e) => Some(Err(e)),
            })
            .collect::<StdResult<Vec<(Addr, Deposit)>>>()?;

        if deposit.is_empty() {
            return Err(StdError::GenericErr {
                msg: format!("No deposit with given ID was found: {}", search_id),
            });
        }
        if deposit.len() != 1 {
            return Err(StdError::GenericErr {
                msg: format!(
                    "Something went wrong; More then 1 deposit with searched ID: {}",
                    search_id
                ),
            });
        }

        Ok(DepositByIdResponse {
            sender: deposit[0].0.clone(),
            deposit: deposit[0].1.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::CosmosMsg;

    #[test]
    fn exchange_messages() {
        let deposit = (
            Addr::unchecked("first"),
            Asset::new_native(100_000u128, "ujuno"),
        );
        let exchange = (
            Addr::unchecked("second"),
            Asset::new_native(200_000u128, "uusdc"),
        );

        let exchanges =
            execute::create_exchange_messages(&deposit.0, &exchange.1, &exchange.0, &deposit.1)
                .unwrap();

        assert_eq!(
            exchanges,
            vec![
                CosmosMsg::Bank(BankMsg::Send {
                    to_address: "first".to_owned(),
                    amount: coins(200_000u128, "uusdc")
                }),
                CosmosMsg::Bank(BankMsg::Send {
                    to_address: "second".to_owned(),
                    amount: coins(100_000u128, "ujuno")
                })
            ]
        );

        let exchange = (
            Addr::unchecked("second"),
            Asset::new_cw20(200_000u128, "tokenaddress"),
        );

        let exchanges =
            execute::create_exchange_messages(&deposit.0, &exchange.1, &exchange.0, &deposit.1)
                .unwrap();

        assert_eq!(
            exchanges,
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "tokenaddress".to_owned(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: "first".to_owned(),
                        amount: 200_000u128.into()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                CosmosMsg::Bank(BankMsg::Send {
                    to_address: "second".to_owned(),
                    amount: coins(100_000u128, "ujuno")
                })
            ]
        );

        let deposit = (
            Addr::unchecked("first"),
            Asset::new_cw20(100_000u128, "othertoken"),
        );

        let exchanges =
            execute::create_exchange_messages(&deposit.0, &exchange.1, &exchange.0, &deposit.1)
                .unwrap();

        assert_eq!(
            exchanges,
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "tokenaddress".to_owned(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: "first".to_owned(),
                        amount: 200_000u128.into()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "othertoken".to_owned(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: "second".to_owned(),
                        amount: 100_000u128.into()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        );
    }
}
