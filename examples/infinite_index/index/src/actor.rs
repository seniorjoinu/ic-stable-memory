use candid::Principal;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::SBTreeMap;
use ic_stable_memory::utils::DebuglessUnwrap;
use ic_stable_memory::{
    retrieve_custom_data, stable_memory_init, stable_memory_post_upgrade,
    stable_memory_pre_upgrade, store_custom_data, SBox,
};
use std::cell::RefCell;

mod v1;
use v1::{User, UserLatest};

/*
mod v2;
use v2::{User, UserLatest};
 */
#[update]
fn create_user(user_payload: UserLatest) {
    with_state(|state| {
        let id = user_payload.id;
        let user = User::new(user_payload);
        let boxed_user = SBox::new(user).expect("Out of memory");

        state.insert(id, boxed_user).expect("Out of memory");
    })
}

#[query]
fn get_user(id: Principal) -> Option<User> {
    with_state(|state| state.get(&id).map(|it| it.as_latest()))
}

type State = SBTreeMap<Principal, SBox<User>>;

thread_local! {
    static STATE: RefCell<Option<State>> = RefCell::default();
}

pub fn with_state<R, F: FnOnce(&mut State) -> R>(f: F) -> R {
    STATE.with(|s| {
        let mut state_ref = s.borrow_mut();
        let state = state_ref.as_mut().unwrap();

        f(state)
    })
}

#[init]
fn init() {
    stable_memory_init();

    STATE.with(|s| s.replace(Some(SBTreeMap::default())));
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
