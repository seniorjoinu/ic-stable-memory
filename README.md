# IC stable memory

With this library you can:
* use __stable variables__ in your code - they store their data completely in stable memory, so you don't have to do your regular routine serializing/deserializing them in `pre_updage`/`post_upgrade` hooks
* use __stable collections__, like `SVec` and `SHashMap` which work directly with stable memory and are able to hold as many data as the subnet would allow your canister to hold

### Pros:
1. Use all the memory, which your canister's subnet can provide (additional to 4GB of heap you already have).
2. Still be able to upgrade your canister.

### Cons:
1. Your canister will consume more cycles, than usual, since it now does a lot of system calls in order to use stable memory.
2. It is a early version software, so there may be bugs. This will improve in future. Please, report if you've encountered one.

## Installation
```toml
# cargo.toml

[dependencies]
ic-stable-memory = "0.0.1"
```

## Quick example
Check out [the example project](./examples/token) to find out more.

Let's suppose, you have a vector of strings, which you want to persist between canister upgrades. For every data chunk which is small
enough (so it would be cheap to serialize/deserialize it every time you use it) , you can use __stable variables__ to store it in stable memory:

```rust
use ic_stable_memory::utils::mem_context::stable;
use ic_stable_memory::utils::vars::{get_var, init_vars, reinit_vars, set_var, store_vars};
use ic_stable_memory::{
    init_allocator, reinit_allocator,
};

type MyStrings = Vec<String>;

#[init]
fn init() {
    // initialize the library
    
    // grow you stable memory (if it wasn't used before) for at least one page
    stable::grow(1).expect("Out of memory");
    
    // initialize the stable memory allocator
    init_allocator(0);
    
    // initialize the stable variables collection
    init_vars();

    // create the stable variable
    set_var("my_strings", &MyStrings::new()).expect("Unable to create my_strings stable var");
}


#[pre_upgrade]
fn pre_upgrade() {
    // save stable variables meta (your data is already in stable memory, but you have to save the pointer to it, so it could be found after the upgrade)
    store_vars();
}

#[post_upgrade]
fn post_upgrade() {
    // reinitialize stable memory and variables (it's cheap)
    reinit_allocator(0);
    reinit_vars();
}

#[query]
fn get_my_strings() -> MyStrings {
    get_var::<MyStrings>("my_strings")
}

#[update]
fn add_my_string(entry: String) {
    let mut my_strings = get_var::<MyStrings>("my_strings");
    my_strings.push(entry);
    
    set_var("my_strings", &my_strings).expect("Out of memory");
}
```

This would work fine for any kind of small data, like settings. But when you need to store bigger data, it may be really 
inefficient to serialize/deserialize gigabytes of data just to read a couple of kilobytes from it. For example, if you're
storing some kind of an event log (which can grow into a really big thing), you only want to access some limited number of
entries at a time. In this case, you want to use a __stable collection__.

```rust
use ic_stable_memory::collections::vec::SVec;
use ic_stable_memory::utils::mem_context::stable;
use ic_stable_memory::utils::vars::{get_var, init_vars, reinit_vars, set_var, store_vars};
use ic_stable_memory::{
    init_allocator, reinit_allocator,
};

// Note, that Vec transformed into SVec
type MyStrings = SVec<String>;
type MyStringsSlice = Vec<String>;

#[init]
fn init() {
    // this init function body looks the same as it was in the previous example, but now we create a different stable_variable
    
    stable::grow(1).expect("Out of memory");
    init_allocator(0);
    
    // we still have to use a stable variable in order to save SVec's pointer in it, to persist it between upgrades
    init_vars();

    // now, our stable variable will hold an SVec pointer instead of the the whole Vec as it was previously 
    set_var("my_strings", &MyStrings::new()).expect("Unable to create my_strings stable var");
}

#[pre_upgrade]
fn pre_upgrade() {
    // the same as before
    store_vars();
}

#[post_upgrade]
fn post_upgrade() {
    // the same as before
    reinit_allocator(0);
    reinit_vars();
}

#[query]
fn get_my_strings_page(from: u64, to: u64) -> MyStringsSlice {
    let my_strings = get_var::<MyStrings>("my_strings");
    
    // our stable collection can be very big, so we only return a page of it
    let mut result = MyStringsSlice::new();
    
    for i in from..to {
        let entry: String = my_strings.get_cloned(&i).expect(format!("No entry at pos {}", i).as_str());
        result.push(entry);
    }
    
    result
}

#[update]
fn add_my_string(entry: String) {
    let mut my_strings = get_var::<MyStrings>("my_strings");
    
    // this call now pushes new value directly to stable memory
    my_strings.push(entry).expect("Out of memory");

    // only saves SVec's meta, instead of the whole collection
    set_var("my_strings", &my_strings).expect("Out of memory");
}
```

There is also a `SHashMap` collection, if you need keyed values.
