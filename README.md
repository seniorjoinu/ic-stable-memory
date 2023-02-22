![test coverage 88.82%](https://badgen.net/badge/coverage/88.82%25/green)


# IC Stable Memory

Allows using canister's stable memory as main memory.

## Features
* `8` stable data structures:
  * `SBox` in replacement for `Box`
  * `SVec` and `SLog` in replacement for `Vec`
  * `SHashMap` in replacement for `HashMap`
  * `SHashSet` in replacement for `HashSet`
  * `SBTreeMap` in replacement for `BTreeMap`
  * `SBTreeSet` in replacement for `BTreeSet`
  * `SCertifiedBTreeMap` in replacement for Dfinity's `RBTree`
* Enforced Rust's borrower rules: 
  * data structures drop automatically when leaving the scope
  * data structures own their inner values, allowing by-reference access
* The API allows programmatic reaction to `OutOfMemory` errors, while keeping it almost identical to `std`
* Complete toolset to build your own stable data structure

## Installation
```toml
# cargo.toml

[dependencies]
ic-stable-memory = "0.4"
```

## Quick example
Let's build a `Todo` app, since they're very popular :)

```rust
use candid::{CandidType, Deserialize};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::SVec;
use ic_stable_memory::derive::{CandidAsDynSizeBytes, StableType};
use ic_stable_memory::{
  retrieve_custom_data, stable_memory_init, stable_memory_post_upgrade,
  stable_memory_pre_upgrade, store_custom_data, SBox,
};
use std::cell::RefCell;

#[derive(CandidType, Deserialize, StableType, CandidAsDynSizeBytes, Debug, Clone)]
struct Task {
  title: String,
  description: String,
}

// If you can implement AsFixedSizeBytes for your data type, 
// you can store it directly, without wrapping in SBox
type State = SVec<SBox<Task>>;

thread_local! {
  static STATE: RefCell<Option<State>> = RefCell::default();
}

#[init]
fn init() {
  stable_memory_init();

  STATE.with(|s| {
    *s.borrow_mut() = Some(SVec::new());
  });
}

#[pre_upgrade]
fn pre_upgrade() {
  let state: State = STATE.with(|s| s.borrow_mut().take().unwrap());
  let boxed_state = SBox::new(state).expect("Out of memory");

  store_custom_data(0, boxed_state);

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

#[update]
fn add_task(task: Task) {
  STATE.with(|s| {
    let boxed_task = SBox::new(task).expect("Out of memory");
    s.borrow_mut()
            .as_mut()
            .unwrap()
            .push(boxed_task)
            .expect("Out of memory");
  });
}

#[update]
fn remove_task(idx: u32) {
  STATE.with(|s| {
    s.borrow_mut().as_mut().unwrap().remove(idx as usize);
  });
}

#[update]
fn swap_tasks(idx_1: u32, idx_2: u32) {
  STATE.with(|s| {
    s.borrow_mut()
            .as_mut()
            .unwrap()
            .swap(idx_1 as usize, idx_2 as usize);
  });
}

#[query]
fn get_todo_list() -> Vec<Task> {
  STATE.with(|s| {
    let mut result = Vec::new();

    for task in s.borrow().as_ref().unwrap().iter() {
      result.push(task.clone());
    }

    result
  })
}
```

## Documentation
1. [Complete API documentation](https://docs.rs/ic-stable-memory/)
2. [How to migrate from standard data structures](./docs/migration.md)
3. [How to handle OutOfMemory errors](./docs/out-of-memory-error-handling.md)
4. [How to ensure data upgradability](./docs/upgradeability.md)
5. [How to implement encoding traits](./docs/encoding.md)
6. [Performance tips](./docs/perfomance.md)
7. [Benchmarks](./docs/benchmarks.md)
8. [How to build your own stable data structure](./docs/user-defined-data-structures.md)

## Example projects
* [Simple token canister](./examples/token)
* [Performance counter canister](./examples/performance_counter)
* [Stable certified assets canister](https://github.com/seniorjoinu/ic-stable-certified-assets)

## Contribution
This is an emerging software, so any help is greatly appreciated.
Feel free to propose PR's, architecture tips, bug reports or any other feedback.

## Test coverage check
* `cargo install grcov`
* `rustup component add llvm-tools-preview`
* `./coverage.sh --test` (won't rebuild without `--test`)