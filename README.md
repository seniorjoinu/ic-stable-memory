THIS IS __NOT__ A BATTLE-TESTED SOFTWARE; USE AT YOUR OWN RISK

![test coverage 72.64%](https://badgen.net/badge/coverage/72.64%25/yellow)

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
ic-stable-memory = "0.2.0"
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
    s! { MyStrings = MyStrings::new() };
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
    
    s! { MyStrings = my_strings };
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
    s! { MyStrings = MyStrings::new() };
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
    my_strings.push(entry);

    // only saves SVec's pointer, instead of the whole collection
    s! { MyStrings = my_strings };
}
```

## Horizontal scaling
Using this library you can utilize the maximum of your canister's memory. Instead of 4GB of heap memory,
you're now able to use up to 8GBs of stable memory, which is twice more available memory per a single canister.

And this is good, when you know that your canister will store only some limited amount of data. But 
what if your data set size is unknown and theoretically can be really big (like, terabytes)? There is 
only one way to handle this situation - to scale horizontally.

And ic-stable-memory helps with that a little bit. There is a special configuration parameter:
```rust
fn get_max_allocation_pages() -> u32;
fn set_max_allocation_pages(pages: u32);
```
This parameter defines how much of free stable memory the library should always keep. By default it is set
to `180 pages` (~10MB). This means that the library will always make sure, that your 
canister have this amount of memory available no matter what. This is important, since on the IC all memory
gets allocated to canisters on-demand.

When the subnet won't be able to give your canister enough memory to fulfill this parameter (two reasons: 1. 
subnet is out of memory at all; 2. your canister reached its memory limits), a special function of your canister
will be invoked:

```rust
#[update]
fn on_low_stable_memory() {
    // do whatever you need to do, when your canister is out of memory
}
```

This function is named `on_low_stable_memory()` and has to have no arguments or return values. Inside
this function you can:
* spawn a new canister to scale horizontally;
* block your canister from accepting new requests;
* send messages to some logging service;
* etc.

In other words, you can do whatever you want in order to keep your service operable even if the canister
is out of stable memory.

##### ! Important !

This function will only be called __ONCE__! If you forgot to define it and ran out of memory - it won't work
for you anymore, even if you add it to the canister later.

## Collections

### SVec
[source code](./src/collections/vec.rs)

// TODO: API

### SHashMap
[source code](./src/collections/hash_map.rs)

// TODO: API

### SHashSet
[source code](./src/collections/hash_set.rs)

// TODO: API

### SBinaryHeap
[source code](./src/collections/binary_heap.rs)

// TODO: API

### SBTreeMap
[source code](./src/collections/btree_map.rs)

// TODO: API

### SBTreeSet
[source code](./src/collections/btree_set.rs)

// TODO: API


## Benchmarks
### Vec
```
"Classic vec push" 1000000 iterations: 264 ms
"Stable vec push" 1000000 iterations: 98733 ms (x374 slower)

"Classic vec pop" 1000000 iterations: 153 ms
"Stable vec pop" 1000000 iterations: 84355 ms (x551 slower)

"Classic vec search" 1000000 iterations: 47 ms
"Stable vec search" 1000000 iterations: 27294 ms (x580 slower)
```

### Binary heap
```
"Classic binary heap push" 1000000 iterations: 398 ms
"Stable binary heap push" 1000000 iterations: 139906 ms (x352 slower)

"Classic binary heap pop" 1000000 iterations: 2025 ms
"Stable binary heap pop" 1000000 iterations: 180150 ms (x88 slower)

"Classic binary heap peek" 1000000 iterations: 55 ms
"Stable binary heap peek" 1000000 iterations: 26512 ms (x482 slower)
```

### Hash map
```
"Classic hash map insert" 100000 iterations: 165 ms
"Stable hash map insert" 100000 iterations: 92284 ms (x559 slower)

"Classic hash map remove" 100000 iterations: 86 ms
"Stable hash map remove" 100000 iterations: 75013 ms (x872 slower)

"Classic hash map search" 100000 iterations: 52 ms
"Stable hash map search" 100000 iterations: 70477 ms (x1355 slower)
```


## TODO
Make it use speedy

## Contribution
This is an emerging software, so any help is greatly appreciated.
Feel free to propose PR's, architecture tips, bug reports or any other feedback.

You can reach me out via [Telegram](https://t.me/joinu14), if I don't answer here for too long.