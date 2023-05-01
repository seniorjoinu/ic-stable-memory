use candid::{encode_one, Nat, Principal};
use child::{IIndexCanister, InitReq, Side};
use ic_cdk::id;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::{SBTreeMap, SVec};
use ic_stable_memory::derive::{AsFixedSizeBytes, StableType};
use ic_stable_memory::utils::DebuglessUnwrap;
use ic_stable_memory::{
    retrieve_custom_data, stable_memory_init, stable_memory_post_upgrade,
    stable_memory_pre_upgrade, store_custom_data, OutOfMemory, SBox,
};
use management::*;
use std::cell::RefCell;

mod child;
mod management;

pub type Key = Nat;
pub type Value = String;

#[derive(Clone, AsFixedSizeBytes, StableType, Eq)]
pub struct Child {
    pub min_key: Key,
    pub max_key: Key,
    pub id: Principal,
}

impl PartialOrd for Child {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.min_key.partial_cmp(&other.min_key)
    }
}

impl PartialEq for Child {
    fn eq(&self, other: &Self) -> bool {
        self.min_key.eq(&other.min_key)
    }
}

#[derive(Default, AsFixedSizeBytes, StableType)]
struct State {
    child_wasm_module: SVec<u8>,
    children: SVec<Child>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = RefCell::default();
}

impl State {
    pub async fn spawn_child(&self, init_req: InitReq) -> Principal {
        let req = CreateCanisterRequest {
            settings: Some(DeployCanisterSettings {
                controller: Some(id()),
                compute_allocation: None,
                memory_allocation: None,
                freezing_threshold: None,
            }),
        };

        let cycles = 2_000_000_000_000;

        let (CreateCanisterResponse { canister_id },) = Principal::management_canister()
            .create_canister(req, cycles)
            .await
            .expect("Unable to spawn new canister");

        Principal::management_canister()
            .install_code(InstallCodeRequest {
                canister_id: canister_id,
                mode: CanisterInstallMode::install,
                wasm_module: self.child_wasm_module.as_std_blob(),
                arg: encode_one(init_req).unwrap(),
            })
            .await
            .expect("Unable to install wasm to new canister");

        canister_id
    }
}

#[update]
async fn set(key: Key, value: Value) -> Option<Value> {
    with_state(|state| async {
        if state.children.is_empty() {
            let child_id = state.spawn_child(InitReq::empty()).await;

            let child = Child {
                min_key: key.clone(),
                max_key: key,
                id: child_id,
            };

            state.children.push(child).debugless_unwrap();

            let res = child_id
                .set(&key, &value)
                .await
                .debugless_unwrap()
                .is_none();

            assert!(res, "no previous value should exist");

            return None;
        }

        match state.children.binary_search_by(|it| it.min_key.cmp(&key)) {
            Ok(idx) => {
                // we need this scope in order to apply modifications to the child
                let e = {
                    let mut child = state.children.get_mut(idx).unwrap();

                    let e = match child.set(&key, &value).await {
                        Ok(res) => {
                            if key > child.max_key {
                                child.max_key = key;
                            }
                            if key < child.min_key {
                                child.min_key = key;
                            }

                            return res;
                        }
                        Err(e) => e,
                    };

                    match e.side {
                        Side::Less => {
                            child.min_key = e.new_limit_key;
                        }
                        Side::More => {
                            child.max_key = e.new_limit_key;
                        }
                    };

                    e
                };

                let new_child_min_key = e.data_chunk.first().map(|it| it.0.clone()).unwrap();
                let new_child_max_key = e.data_chunk.last().map(|it| it.0.clone()).unwrap();

                let child_id = state.spawn_child(InitReq::new(e.data_chunk)).await;

                let new_child = Child {
                    min_key: new_child_min_key,
                    max_key: new_child_max_key,
                    id: child_id,
                };

                match state
                    .children
                    .binary_search_by(|it| it.min_key.cmp(&new_child.min_key))
                {
                    Ok(_) => unreachable!("There shouldn't be any child with these keys"),
                    Err(idx) => state.children.insert(idx, new_child).debugless_unwrap(),
                };
            }
        }

        if state.children.len() == 1 {
            let child = state.children.get(0).unwrap();
            match child.set(&key, &value).await {
                Ok(prev_val) => return prev_val,
                Err(_) => {
                    let child_id = state.spawn_child().await;
                }
            }
        }
    })
    .await
}

#[query]
fn get(key: Key) -> Option<Value> {
    with_state(|state| state.get(&key).map(|it| it.clone()))
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

    STATE.with(|s| s.replace(Some(State::default())));
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
