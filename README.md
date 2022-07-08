# IC Stable Memory

With this Rust library you can:
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
ic-stable-memory = "0.0.2"
```

## Quick example
Check out [the example project](./examples/token) to find out more.

Also, read these articles:
* [IC Stable Memory Library Introduction](https://suvtk-3iaaa-aaaal-aavfa-cai.raw.ic0.app/d/ic-stable-memory-library-introduction)
* [IC Stable Memory Library Under The Hood](https://suvtk-3iaaa-aaaal-aavfa-cai.raw.ic0.app/d/ic-stable-memory-library-under-the-hood)
* [Building A Token Canister With IC Stable Memory Library](https://suvtk-3iaaa-aaaal-aavfa-cai.raw.ic0.app/d/building-a-token-canister-with-ic-stable-memory-library)

Let's suppose, you have a vector of strings, which you want to persist between canister upgrades. For every data chunk which is small
enough (so it would be cheap to serialize/deserialize it every time you use it) , you can use __stable variables__ to store it in stable memory.

```rust
// Define a separate type for the data you want to store in stable memory.
// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
// !! This is important, otherwise macros won't work! !!
// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
// Here we use String type, but any CandidType would work just fine
type MyStrings = Vec<String>;

#[init]
fn init() {
    stable_memory_init(true, 0);

    // create the stable variable
    s!(MyStrings = MyStrings::new()).expect("Unable to create my_strings stable var");
}

#[pre_upgrade]
fn pre_upgrade() {
    stable_memory_pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade(0);
}

#[query]
fn get_my_strings() -> MyStrings {
    s!(MyStrings)
}

#[update]
fn add_my_string(entry: String) {
    let mut my_strings = s!(MyStrings);
    my_strings.push(entry);
    
    s!(MyStrings = my_strings).expect("Out of memory");
}
```

This would work fine for any kind of small data, like settings. But when you need to store bigger data, it may be really 
inefficient to serialize/deserialize gigabytes of data just to read a couple of kilobytes from it. For example, if you're
storing some kind of an event log (which can grow into a really big thing), you only want to access some limited number of
entries at a time. In this case, you want to use a __stable collection__.

```rust
// Note, that Vec transformed into SVec
// again, any CandidType will work
type MyStrings = SVec<String>;
type MyStringsSlice = Vec<String>;

#[init]
fn init() {
    stable_memory_init(true, 0);

    // now, our stable variable will hold an SVec pointer instead of the the whole Vec as it was previously
    s!(MyStrings = MyStrings::new()).expect("Unable to create my_strings stable var");
}

#[pre_upgrade]
fn pre_upgrade() {
    stable_memory_pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade(0);
}

#[query]
fn get_my_strings_page(from: u64, to: u64) -> MyStringsSlice {
    let my_strings = s!(MyStrings);
    
    // our stable collection can be very big, so we only return a page of it
    let mut result = MyStringsSlice::new();
    
    for i in from..to {
        let entry: String = my_strings.get_cloned(i).expect(format!("No entry at pos {}", i).as_str());
        result.push(entry);
    }
    
    result
}

#[update]
fn add_my_string(entry: String) {
    let mut my_strings = s!(MyStrings);
    
    // this call now pushes new value directly to stable memory
    my_strings.push(entry).expect("Out of memory");

    // only saves SVec's pointer, instead of the whole collection
    s!(MyStrings = my_strings).expect("Out of memory");
}
```

There is also a `SHashMap` collection, if you need keyed values.

## Contribution
This is an emerging software, so any help is greatly appreciated.
Feel free to propose PR's, architecture tips, bug reports or any other feedback.

You can reach me out via [Telegram](https://t.me/joinu14), if I don't answer here for too long.