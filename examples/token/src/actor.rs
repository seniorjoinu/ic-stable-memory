use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::time;
use ic_cdk::caller;
use ic_cdk_macros::{heartbeat, init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::collections::vec::SVec;
use ic_stable_memory::utils::mem_context::{stable, PAGE_SIZE_BYTES};
use ic_stable_memory::utils::vars::{get_var, init_vars, reinit_vars, set_var, store_vars};
use ic_stable_memory::{
    _debug_print_allocator, get_allocated_size, get_free_size, init_allocator, reinit_allocator,
};

type Token = SHashMap<Principal, u64>;
type Ledger = SVec<HistoryEntry>;

#[derive(CandidType, Deserialize)]
struct HistoryEntry {
    pub from: Option<Principal>,
    pub to: Option<Principal>,
    pub qty: u64,
    pub timestamp: u64,
}

const TOKEN: &str = "token";
const LEDGER: &str = "ledger";
const TOTAL_SUPPLY: &str = "total_supply";

#[init]
fn init() {
    // initialize stable memory (cheap)
    stable::grow(1).expect("Out of memory");
    init_allocator(0);
    init_vars();

    // initialize stable variables (cheap)
    set_var(TOKEN, &Token::new()).expect("Unable to create token stable var");
    set_var(TOTAL_SUPPLY, &0u64).expect("Unable to create total_supply stable var");
    set_var(LEDGER, &Ledger::new()).expect("Unable to create ledger stable var")
}

#[pre_upgrade]
fn pre_upgrade() {
    // save stable variables meta (cheap)
    store_vars();
}

#[post_upgrade]
fn post_upgrade() {
    // reinitialize stable memory and variables (cheap)
    reinit_allocator(0);
    reinit_vars();
}

#[update]
fn mint(to: Principal, qty: u64) {
    // update token
    let mut token = get_var::<Token>(TOKEN);
    let balance = token.get_cloned(&to).unwrap_or_default();

    token
        .insert(to, balance + qty)
        .expect("Unable to add new token entry");
    set_var(TOKEN, &token).expect("Unable to save token");

    // update total supply
    let total_supply = get_var::<u64>(TOTAL_SUPPLY);
    set_var(TOTAL_SUPPLY, &(total_supply + qty)).expect("Unable to save total supply");

    // emit ledger entry
    let entry = HistoryEntry {
        from: None,
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = get_var::<Ledger>(LEDGER);
    ledger
        .push(&entry)
        .expect("Unable to push new history entry");
    set_var(LEDGER, &ledger).expect("Unable to save ledger");
}

#[update]
fn transfer(to: Principal, qty: u64) {
    let from = caller();

    // update token
    let mut token = get_var::<Token>(TOKEN);

    let from_balance = token.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");
    token
        .insert(from, from_balance - qty)
        .expect("Unable to add new token entry");

    let to_balance = token.get_cloned(&to).unwrap_or_default();
    token
        .insert(to, to_balance + qty)
        .expect("Unable to add new token entry");

    set_var(TOKEN, &token).expect("Unable to save token");

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: Some(to),
        qty,
        timestamp: time(),
    };
    let mut ledger = get_var::<Ledger>(LEDGER);
    ledger
        .push(&entry)
        .expect("Unable to push new history entry");
    set_var(LEDGER, &ledger).expect("Unable to save ledger");
}

#[update]
fn burn(qty: u64) {
    let from = caller();

    // update token
    let mut token = get_var::<Token>(TOKEN);
    let from_balance = token.get_cloned(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");

    token
        .insert(from, from_balance - qty)
        .expect("Unable to add new token entry");
    set_var(TOKEN, &token).expect("Unable to save token");

    let total_supply = get_var::<u64>(TOTAL_SUPPLY);
    set_var(TOTAL_SUPPLY, &(total_supply - qty)).expect("Unable to save token");

    // emit ledger entry
    let entry = HistoryEntry {
        from: Some(from),
        to: None,
        qty,
        timestamp: time(),
    };
    let mut ledger = get_var::<Ledger>(LEDGER);
    ledger
        .push(&entry)
        .expect("Unable to push new history entry");
    set_var(LEDGER, &ledger).expect("Unable to save ledger");
}

#[query]
fn balance_of(of: Principal) -> u64 {
    get_var::<Token>(TOKEN).get_cloned(&of).unwrap_or_default()
}

#[query]
fn total_supply() -> u64 {
    get_var::<u64>(TOTAL_SUPPLY)
}

#[query]
fn get_history(page_index: u64, page_size: u64) -> (Vec<HistoryEntry>, u64) {
    let from = page_index * page_size;
    let to = page_index * page_size + page_size;

    let ledger = get_var::<Ledger>(LEDGER);
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

#[heartbeat]
fn tick() {
    for _ in 0..100 {
        mint(Principal::anonymous(), 1);
    }
    _debug_print_allocator();
}
