use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Order, StdResult, Storage, Uint128};
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
    let id = ID_COUNT.may_load(store)?.unwrap_or_default();
    ID_COUNT.save(store, &(id + 1))?;
    Ok(id)
}

pub const DEPOSITS: Map<(&Addr, ID), Deposit> = Map::new("deposits");

pub fn add_deposit(storage: &mut dyn Storage, sender: &Addr, deposit: &Deposit) -> StdResult<()> {
    let id = next_id(storage)?;
    DEPOSITS.save(storage, (sender, id), &deposit)
}

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

#[cw_serde]
pub struct FeeConfig {
    pub fee_address: String,
    pub optional_service_fee: Decimal,
}

pub const FEE_CONFIG: Item<FeeConfig> = Item::new("fee_config");
