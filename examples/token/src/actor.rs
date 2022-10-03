use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::time;
use ic_cdk::{caller, print};
use ic_cdk_macros::{heartbeat, init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::collections::vec_direct::SVec;
use ic_stable_memory::utils::ic_types::SPrincipal;
use ic_stable_memory::{
    get_allocated_size, get_free_size, s, set_max_grow_pages, stable, stable_memory_init,
    stable_memory_post_upgrade, stable_memory_pre_upgrade, PAGE_SIZE_BYTES,
};
use speedy::{Readable, Writable};

type AccountBalances = SHashMap<SPrincipal, u64>;
type TransactionLedger = SVec<HistoryEntry>;
type TotalSupply = u64;

#[derive(CandidType, Deserialize, Readable, Writable)]
struct HistoryEntry {
    pub from: Option<SPrincipal>,
    pub to: Option<SPrincipal>,
    pub qty: u64,
    pub timestamp: u64,
}

#[update]
fn mint(to: SPrincipal, qty: u64) {
    // update balances
    let mut balances = s!(AccountBalances);
    let balance = balances.get_cloned(&to).unwrap_or_default();

    balances.insert(to, &(balance + qty));

    s! { AccountBalances = balances };

    // update total supply
    let total_supply: u64 = s!(TotalSupply);
    s! { TotalSupply = total_supply + qty };

    // emit ledger entry
    let entry = HistoryEntry {
        from: None,
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger.push(&entry);

    s! { TransactionLedger = ledger };
}

#[update]
fn transfer(to: SPrincipal, qty: u64) {
    let from = SPrincipal(caller());

    // update balances
    let mut balances = s!(AccountBalances);

    let from_balance = balances.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");
    balances.insert(from, &(from_balance - qty));

    let to_balance = balances.get_cloned(&to).unwrap_or_default();
    balances.insert(to, &(to_balance + qty));

    s! { AccountBalances = balances };

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger.push(&entry);

    s! { TransactionLedger = ledger };
}

#[update]
fn burn(qty: u64) {
    let from = SPrincipal(caller());

    // update balances
    let mut balances = s!(AccountBalances);
    let from_balance = balances.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");

    balances.insert(from, &(from_balance - qty));
    s! { AccountBalances = balances };

    let total_supply = s!(TotalSupply);
    s! { TotalSupply = total_supply - qty };

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: None,
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger.push(&entry);

    s! { TransactionLedger = ledger };
}

#[query]
fn balance_of(of: SPrincipal) -> u64 {
    s!(AccountBalances).get_cloned(&of).unwrap_or_default()
}

#[query]
fn total_supply() -> TotalSupply {
    s!(TotalSupply)
}

#[query]
fn get_history(page_index: u64, page_size: u64) -> (Vec<HistoryEntry>, u64) {
    let from = page_index * page_size;
    let to = page_index * page_size + page_size;

    let ledger = s!(TransactionLedger);
    let mut result = vec![];
    let total_pages = ledger.len() / page_size + 1;

    for i in from..to {
        if let Some(entry) = ledger.get_cloned(i) {
            result.push(entry);
        }
    }

    (result, total_pages)
}

#[query]
fn mem_metrics() -> (u64, u64, u64) {
    (
        stable::size_pages() * PAGE_SIZE_BYTES as u64, // available
        get_allocated_size(),                          // allocated
        get_free_size(),                               // free
    )
}

#[init]
fn init() {
    // initialize stable memory (cheap)
    stable_memory_init(true, 0);
    set_max_grow_pages(200);

    // initialize stable variables (cheap)
    s! { AccountBalances = AccountBalances::new() };
    s! { TransactionLedger = TransactionLedger::new() };
    s! { TotalSupply = TotalSupply::default() };
}

#[pre_upgrade]
fn pre_upgrade() {
    // save stable variables meta-info (cheap)
    stable_memory_pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    // reinitialize stable memory and variables (cheap)
    stable_memory_post_upgrade(0);
}

#[heartbeat]
fn tick() {
    for _ in 0..100 {
        mint(SPrincipal(Principal::management_canister()), 1000);
    }
}

// ON LOW MEMORY CALLBACK
#[update]
fn on_low_stable_memory() {
    print("!!! CANISTER IS LOW ON STABLE MEMORY !!!");
    print(format!(
        "total allocated: {} bytes, total free: {} bytes",
        get_allocated_size(),
        get_free_size()
    ));
}
