use candid::{CandidType, Principal};
use ic_cdk::caller;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::primitive::s_unsafe_cell::SUnsafeCell;
use ic_stable_memory::utils::mem_context::{stable, OutOfMemory};
use ic_stable_memory::utils::vars::{get_var, init_vars, reinit_vars, set_var, store_vars};
use ic_stable_memory::{
    _get_custom_data_ptr, _set_custom_data_ptr, init_allocator, reinit_allocator,
};
use serde::de::DeserializeOwned;

type Token = SHashMap<Principal, u64>;

#[init]
fn init() {
    stable::grow(1).expect("Out of memory");
    init_allocator(0);
    init_vars();

    set_var("token", &Token::new()).expect("Unable to init");
    set_var("total_supply", &0u64).expect("Unable to init");
}

#[pre_upgrade]
fn pre_upgrade() {
    store_vars();
}

#[post_upgrade]
fn post_upgrade() {
    reinit_allocator(0);
    reinit_vars();
}

#[update]
fn mint(to: Principal, qty: u64) {
    let mut token = get_var::<Token>("token");
    let total_supply = get_var::<u64>("total_supply");

    let balance = token.get_cloned(&to).unwrap_or_default();
    token.insert(to, balance + qty).expect("Out of memory1");

    set_var("token", &token).expect("Unable to mint");
    set_var("total_supply", &(total_supply + qty)).expect("Unable to mint");
}

#[update]
fn transfer(to: Principal, qty: u64) {
    let from = caller();
    let mut token = get_var::<Token>("token");

    let from_balance = token.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");

    token.insert(from, from_balance - qty).unwrap();

    let to_balance = token.get_cloned(&to).unwrap_or_default();
    token.insert(to, to_balance + qty).expect("Out of memory");

    set_var("token", &token).expect("Unable to mint");
}

#[query]
fn balance_of(of: Principal) -> u64 {
    get_var::<Token>("token")
        .get_cloned(&of)
        .unwrap_or_default()
}

#[query]
fn total_supply() -> u64 {
    get_var::<u64>("total_supply")
}

// TODO: make vars api prettier (maybe use TypeId, like they do in cdk_rs)
