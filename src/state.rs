use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Order, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub enum AssetType {
    Native(String),
    Cw20(String),
}

#[cw_serde]
pub struct Asset {
    denom: AssetType,
    amount: Uint128,
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
    let id: ID = ID_COUNT.may_load(store)?.unwrap_or_default() + 1;
    ID_COUNT.save(store, &id)?;
    Ok(id)
}

pub const DEPOSITS: Map<(&Addr, ID), Deposit> = Map::new("deposits");

pub fn get_deposits(storage: &dyn Storage, address: &Addr) -> StdResult<Vec<Deposit>> {
    DEPOSITS
        .prefix(address)
        .range(storage, None, None, Order::Ascending)
        .map(|deposit| {
            let (_, deposit) = deposit?;
            Ok(deposit)
        })
        .collect::<StdResult<Vec<Deposit>>>()
}
