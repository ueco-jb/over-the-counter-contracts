use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Order, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};

use std::fmt;

#[cw_serde]
pub enum AssetType {
    Native(String),
    Cw20(String),
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                AssetType::Native(symbol) => symbol.to_string(),
                AssetType::Cw20(address) => address.to_string(),
            }
        )
    }
}

#[cw_serde]
pub struct Asset {
    pub denom: AssetType,
    pub amount: Uint128,
}

impl Asset {
    pub fn new_native(amount: u128, denom: &str) -> Self {
        Self {
            amount: amount.into(),
            denom: AssetType::Native(denom.to_owned()),
        }
    }

    pub fn new_cw20(amount: u128, denom: &str) -> Self {
        Self {
            amount: amount.into(),
            denom: AssetType::Cw20(denom.to_owned()),
        }
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.denom)
    }
}

#[cw_serde]
pub struct Deposit {
    pub deposit: Asset,
    pub offer: Offer,
}

#[cw_serde]
pub struct Offer {
    pub exchange: Asset,
    pub from: Option<Addr>,
}

pub type ID = u64;

pub const ID_COUNT: Item<ID> = Item::new("id_count");

pub fn next_id(store: &mut dyn Storage) -> StdResult<ID> {
    let id = ID_COUNT.may_load(store)?.unwrap_or_default();
    ID_COUNT.save(store, &(id + 1))?;
    Ok(id)
}

pub const DEPOSITS: Map<(&Addr, ID), Deposit> = Map::new("deposits");

pub fn add_deposit(storage: &mut dyn Storage, sender: &Addr, deposit: &Deposit) -> StdResult<()> {
    let id = next_id(storage)?;
    DEPOSITS.save(storage, (sender, id), deposit)
}

pub fn remove_deposit(storage: &mut dyn Storage, address: &Addr, id: Option<ID>) -> StdResult<()> {
    let keys_to_remove = if let Some(id) = id {
        // If ID is provided, remove only the entry with the provided address and ID
        vec![(address, id)]
    } else {
        // If ID is not provided, remove all entries with the provided address prefix
        DEPOSITS
            .prefix(address)
            .range(storage, None, None, Order::Ascending)
            .map(|item| {
                let (id, _) = item?;
                Ok((address, id))
            })
            .collect::<StdResult<Vec<(&Addr, ID)>>>()?
    };

    for key in keys_to_remove {
        DEPOSITS.remove(storage, key);
    }
    Ok(())
}

pub fn get_deposits(storage: &dyn Storage, address: &Addr) -> StdResult<Vec<(ID, Deposit)>> {
    DEPOSITS
        .prefix(address)
        .range(storage, None, None, Order::Ascending)
        .map(|deposit| {
            let (id, deposit) = deposit?;
            Ok((id, deposit))
        })
        .collect::<StdResult<Vec<(ID, Deposit)>>>()
}

#[cw_serde]
pub struct FeeConfig {
    pub fee_address: Addr,
    pub service_fee: Decimal,
}

pub const FEE_CONFIG: Item<FeeConfig> = Item::new("fee_config");
