use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::time;
use ic_cdk::caller;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::collections::vec::SVec;
use ic_stable_memory::{
    get_allocated_size, get_free_size, s, stable, stable_memory_init, stable_memory_post_upgrade,
    stable_memory_pre_upgrade, PAGE_SIZE_BYTES,
};

type AccountBalances = SHashMap<Principal, u64>;
type TransactionLedger = SVec<HistoryEntry>;
type TotalSupply = u64;

#[derive(CandidType, Deserialize)]
struct HistoryEntry {
    pub from: Option<Principal>,
    pub to: Option<Principal>,
    pub qty: u64,
    pub timestamp: u64,
}

#[init]
fn init() {
    // initialize stable memory (cheap)
    stable_memory_init(true, 0);

    // initialize stable variables (cheap)
    s!(AccountBalances = AccountBalances::new()).expect("Out of memory (balances)");
    s!(TransactionLedger = TransactionLedger::new()).expect("Out of memory (ledger)");
    s!(TotalSupply = TotalSupply::default()).expect("Out of memory (token)");
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

#[update]
fn mint(to: Principal, qty: u64) {
    // update balances
    let mut balances = s!(AccountBalances);
    let balance = balances.get_cloned(&to).unwrap_or_default();

    balances
        .insert(to, balance + qty)
        .expect("Out of memory (balance entry)");

    s!(AccountBalances = balances).expect("Out of memory (balances)");

    // update total supply
    let total_supply: u64 = s!(TotalSupply);
    s!(TotalSupply = total_supply + qty).expect("Out of memory (total supply)");

    // emit ledger entry
    let entry = HistoryEntry {
        from: None,
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger.push(&entry).expect("Out of memory (history entry)");

    s!(TransactionLedger = ledger).expect("Out of memory (ledger)");
}

#[update]
fn transfer(to: Principal, qty: u64) {
    let from = caller();

    // update balances
    let mut balances = s!(AccountBalances);

    let from_balance = balances.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");
    balances
        .insert(from, from_balance - qty)
        .expect("Out of memory (from balance)");

    let to_balance = balances.get_cloned(&to).unwrap_or_default();
    balances
        .insert(to, to_balance + qty)
        .expect("Out of memory (to balance)");

    s!(AccountBalances = balances).expect("Out of memory (token)");

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger
        .push(&entry)
        .expect("Unable to push new history entry");

    s!(TransactionLedger = ledger).expect("Out of memory (ledger)");
}

#[update]
fn burn(qty: u64) {
    let from = caller();

    // update balances
    let mut balances = s!(AccountBalances);
    let from_balance = balances.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");

    balances
        .insert(from, from_balance - qty)
        .expect("Out of memory (balance entry)");
    s!(AccountBalances = balances).expect("Out of memory (token)");

    let total_supply = s!(TotalSupply);
    s!(TotalSupply = total_supply - qty).expect("Out of memory (total supply)");

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: None,
        qty,
        timestamp: time(),
    };
    let mut ledger = s!(TransactionLedger);
    ledger.push(&entry).expect("Out of memory (history entry)");

    s!(TransactionLedger = ledger).expect("Out of memory (ledger)");
}

#[query]
fn balance_of(of: Principal) -> u64 {
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
