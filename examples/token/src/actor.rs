use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::time;
use ic_cdk::caller;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::{SHashMap, SLog};
use ic_stable_memory::derive::{AsFixedSizeBytes, StableType};
use ic_stable_memory::utils::DebuglessUnwrap;
use ic_stable_memory::{
    retrieve_custom_data, stable_memory_init, stable_memory_post_upgrade,
    stable_memory_pre_upgrade, store_custom_data, SBox,
};
use std::cell::RefCell;

#[derive(CandidType, Deserialize, AsFixedSizeBytes, StableType, Debug, Copy, Clone)]
struct HistoryEntry {
    pub from: Option<Principal>,
    pub to: Option<Principal>,
    pub qty: u64,
    pub timestamp: u64,
}

#[derive(StableType, AsFixedSizeBytes)]
struct State {
    balances: SHashMap<Principal, u64>,
    transactions: SLog<HistoryEntry>,
    total_supply: u64,
}

thread_local! {
    static STATE: RefCell<Option<State>> = RefCell::default();
}

#[update]
fn mint(to: Principal, qty: u64) {
    STATE.with(|s| {
        s.borrow_mut().as_mut().unwrap().mint(to, qty, time());
    });
}

#[update]
fn transfer(to: Principal, qty: u64) {
    STATE.with(|s| {
        s.borrow_mut()
            .as_mut()
            .unwrap()
            .transfer(caller(), to, qty, time());
    });
}

#[update]
fn burn(qty: u64) {
    STATE.with(|s| {
        s.borrow_mut().as_mut().unwrap().burn(caller(), qty, time());
    })
}

#[query]
fn balance_of(of: Principal) -> u64 {
    STATE.with(|s| s.borrow().as_ref().unwrap().balance_of(&of))
}

#[query]
fn total_supply() -> u64 {
    STATE.with(|s| s.borrow().as_ref().unwrap().total_supply())
}

#[query]
fn get_history(page_index: usize, page_size: usize) -> (Vec<HistoryEntry>, usize) {
    STATE.with(|s| {
        s.borrow()
            .as_ref()
            .unwrap()
            .get_history(page_index, page_size)
    })
}

#[init]
fn init() {
    stable_memory_init();
}

#[pre_upgrade]
fn pre_upgrade() {
    let state: State = STATE.with(|s| s.borrow_mut().take().unwrap());

    store_custom_data(0, SBox::new(state).debugless_unwrap());

    stable_memory_pre_upgrade().expect("Out of memory");
}

#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade();

    let state = retrieve_custom_data::<State>(0).unwrap().into_inner();

    STATE.with(|s| {
        *s.borrow_mut() = Some(state);
    });
}

impl State {
    fn transfer(&mut self, from: Principal, to: Principal, qty: u64, timestamp: u64) {
        let from_balance_before: u64 = self.balances.get(&from).map(|it| *it).unwrap_or_default();

        assert!(from_balance_before >= qty, "Not enough funds");

        let to_balance_before: u64 = self.balances.get(&to).map(|it| *it).unwrap_or_default();

        let from_balance_after = from_balance_before - qty;
        let to_balance_after = to_balance_before + qty;

        self.balances
            .insert(from, from_balance_after)
            .expect("Out of memory");

        self.balances
            .insert(to, to_balance_after)
            .expect("Out of memory");

        let entry = HistoryEntry {
            from: Some(from),
            to: Some(to),
            qty,
            timestamp,
        };

        self.transactions.push(entry).expect("Out of memory");
    }

    fn mint(&mut self, to: Principal, qty: u64, timestamp: u64) {
        let to_balance = self.balances.get(&to).map(|it| *it).unwrap_or_default();

        self.balances
            .insert(to, to_balance + qty)
            .expect("Out of memory");

        let entry = HistoryEntry {
            from: None,
            to: Some(to),
            qty,
            timestamp,
        };

        self.transactions.push(entry).expect("Out of memory");

        self.total_supply += qty;
    }

    fn burn(&mut self, from: Principal, qty: u64, timestamp: u64) {
        let mut to_balance = if let Some(b) = self.balances.get_mut(&from) {
            b
        } else {
            panic!("Not enough funds");
        };

        if *to_balance < qty {
            panic!("Not enough funds");
        }

        *to_balance -= qty;

        let entry = HistoryEntry {
            from: Some(from),
            to: None,
            qty,
            timestamp,
        };

        self.transactions.push(entry).expect("Out of memory");

        self.total_supply -= qty;
    }

    fn balance_of(&self, of: &Principal) -> u64 {
        self.balances.get(of).map(|it| *it).unwrap_or_default()
    }

    fn total_supply(&self) -> u64 {
        self.total_supply
    }

    fn get_history(&self, page_index: usize, page_size: usize) -> (Vec<HistoryEntry>, usize) {
        let skip = page_index * page_size;
        let take = page_size;
        let total_pages = (self.transactions.len() / page_size as u64) as usize;

        let it = self
            .transactions
            .rev_iter()
            .skip(skip)
            .take(take)
            .map(|it| *it)
            .collect::<Vec<_>>();

        (it, total_pages)
    }
}
