use candid::Principal;
use ic_cdk::caller;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::primitive::s_unsafe_cell::SUnsafeCell;
use ic_stable_memory::utils::mem_context::stable;
use ic_stable_memory::utils::vars::init_stable_vars;
use ic_stable_memory::{
    _get_custom_data_ptr, _set_custom_data_ptr, init_allocator, reinit_allocator, s_declare,
};

static mut TOKEN: Option<SUnsafeCell<SHashMap<Principal, u64>>> = None;

fn get_token() -> &'static mut SHashMap<Principal, u64> {
    unsafe { TOKEN.as_mut().unwrap() }
}

fn set_token(token: SUnsafeCell<SHashMap<Principal, u64>>) {
    unsafe { TOKEN = Some(token) }
}

#[init]
fn init() {
    stable::grow(1).expect("Out of memory");
    init_allocator(0);
    init_stable_vars();

    s_declare!(token = SHashMap::<Principal, u64>::new());

    set_token(token);
}

#[pre_upgrade]
fn pre_upgrade() {
    let cell = SUnsafeCell::new(get_token()).expect("Out of memory");
}

#[post_upgrade]
fn post_upgrade() {
    reinit_allocator(0);

    let cell = _get_custom_data_ptr(0);
    set_token(cell.get_cloned());
}

#[update]
fn mint(to: Principal, qty: u64) {
    let balance = get_token().get(&to).unwrap_or_default();
    get_token()
        .insert(to, balance + qty)
        .expect("Out of memory1");
}

#[update]
fn transfer(to: Principal, qty: u64) {
    let from = caller();
    let from_balance = get_token().get(&from).unwrap_or_default();
    assert!(from_balance >= qty, "Insufficient funds");

    get_token().insert(from, from_balance - qty).unwrap();

    let to_balance = get_token().get(&to).unwrap_or_default();
    get_token()
        .insert(to, to_balance + qty)
        .expect("Out of memory");
}

#[query]
fn balance_of(of: Principal) -> u64 {
    get_token().get(&of).unwrap_or_default()
}
