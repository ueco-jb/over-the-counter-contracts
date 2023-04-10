#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdResult, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::msg::{DepositByIdResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveCw20Msg};
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
            service_fee: Decimal::percent(1),
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
        ExecuteMsg::Receive(cw20_msg) => receive_cw20(deps, info, cw20_msg),
        ExecuteMsg::Deposit { exchange, from } => {
            let funds = info
                .funds
                .first()
                .cloned()
                .ok_or(ContractError::NoFundsWithDeposit {})?;
            execute::deposit(
                deps,
                info.sender,
                Asset::new_native(funds.amount.u128(), &funds.denom),
                exchange,
                from,
            )
        }
        ExecuteMsg::Withdraw { id } => execute::withdraw(deps, info.sender, id),
        ExecuteMsg::AcceptExchange { deposit_id } => {
            let funds = info
                .funds
                .first()
                .cloned()
                .ok_or(ContractError::NoFundsWithDeposit {})?;
            execute::accept_exchange(
                deps,
                info.sender,
                deposit_id,
                Asset::new_native(funds.amount.u128(), &funds.denom),
            )
        }
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&cw20_msg.sender)?;
    match from_binary(&cw20_msg.msg)? {
        ReceiveCw20Msg::Deposit { exchange, from } => execute::deposit(
            deps,
            sender,
            Asset::new_cw20(cw20_msg.amount.u128(), info.sender.as_str()),
            exchange,
            from,
        ),
        ReceiveCw20Msg::AcceptExchange { deposit_id } => execute::accept_exchange(
            deps,
            sender,
            deposit_id,
            Asset::new_cw20(cw20_msg.amount.u128(), info.sender.as_str()),
        ),
    }
}

mod execute {
    use super::*;

    pub fn deposit(
        deps: DepsMut,
        sender: Addr,
        deposit: Asset,
        exchange: Asset,
        from: Option<String>,
    ) -> Result<Response, ContractError> {
        let from = if let Some(from) = from {
            Some(deps.api.addr_validate(&from)?)
        } else {
            None
        };

        let response = Response::new()
            .add_attribute("execute", "deposit")
            .add_attribute("sender", sender.to_string())
            .add_attribute("deposit", deposit.to_string())
            .add_attribute("exchange", exchange.to_string());

        let offer = Deposit {
            deposit,
            offer: Offer { exchange, from },
        };

        add_deposit(deps.storage, &sender, &offer)?;

        Ok(response)
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
        sender: Addr,
        deposit_id: ID,
        offer_funds: Asset,
    ) -> Result<Response, ContractError> {
        let DepositByIdResponse {
            sender: deposit_sender,
            deposit,
        } = query::deposit_by_id(deps.as_ref(), deposit_id)?;

        let exchange_messages = match deposit.offer.exchange.denom.clone() {
            AssetType::Native(denom) => {
                if offer_funds.denom.to_string() == denom {
                    if offer_funds.amount == deposit.offer.exchange.amount {
                        // Create two messages
                        // First sends newly received native funds to the depositor,
                        // second sends original deposit to user that accepted the exchange
                        create_exchange_messages(
                            &deposit_sender,
                            &offer_funds,
                            &sender,
                            &deposit.deposit,
                        )?
                    } else {
                        // User sent incorrect amount of native tokens to accept the exchange
                        return Err(ContractError::ExchangeIncorrectAmount {
                            expected_amount: deposit.offer.exchange.amount,
                            provided_amount: offer_funds.amount,
                        });
                    }
                } else {
                    // User sent incorrect native token to the exchange
                    return Err(ContractError::ExchangeIncorrectDenom {
                        expected: denom,
                        received: offer_funds.denom.to_string(),
                    });
                }
            }
            AssetType::Cw20(denom) => {
                if offer_funds.denom.to_string() == denom {
                    if offer_funds.amount == deposit.offer.exchange.amount {
                        // Create two messages
                        // First sends newly received cw20 funds to the depositor,
                        // second sends original deposit to user that accepted the exchange
                        create_exchange_messages(
                            &deposit_sender,
                            &offer_funds,
                            &sender,
                            &deposit.deposit,
                        )?
                    } else {
                        // User sent incorrect amount of cw20 tokens to accept the exchange
                        return Err(ContractError::ExchangeIncorrectAmount {
                            expected_amount: deposit.offer.exchange.amount,
                            provided_amount: offer_funds.amount,
                        });
                    }
                } else {
                    // User sent incorrect cw20 token to the exchange
                    return Err(ContractError::ExchangeIncorrectDenom {
                        expected: denom,
                        received: offer_funds.denom.to_string(),
                    });
                }
            }
        };

        Ok(Response::new()
            .add_messages(exchange_messages)
            .add_attribute("exchange", "completed")
            .add_attribute("deposit-sender", deposit_sender.to_string())
            .add_attribute("original-deposit", deposit.deposit.to_string())
            .add_attribute("expected", deposit.offer.exchange.to_string())
            .add_attribute("accepted-by", sender.to_string()))
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
