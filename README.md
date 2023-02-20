/// КОВЕРАЖ ДО 90+
/// ассет канистер + токен + перформанс каунтер
///
/// ДОКУМЕНТАЦИЯ
/// Хороший текст для интродакшена
/// 1. Про то, как сделать быстро
/// 2. Про то, как сохранить апгрейдабилити
/// 3. Про то, как хендлить OOM: а) прямой отлов и реверс транзакции; б) композитные структуры данных, бекапящие невлезшие данные в обычные структуры данных; в) удаление старых данных и рестарт транзакции; г) хоризонтал скейлинг
/// 4. Про бенчмарки
/// 5. Про то, как преехать с обычных структур данных, на эти
/// 6. Про енкодинг - как имплементировать Fixed и Dyn + StableType
/// 7. Заглушка про то, как сделать свои собственные структуры данных с помощью этой библиотеки

THIS IS A EARLY SOFTWARE. DON'T USE IN PRODUCTION!


![test coverage 90.42%](https://badgen.net/badge/coverage/90.42%25/green)


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
`Use rust 1.66 nightly or newer`

```toml
# cargo.toml

[dependencies]
ic-stable-memory = "0.4.0-rc1"
```

```rust
// lib.rs

#![feature(thread_local)]
#![feature(generic_const_exprs)]
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
// Here we use String type, but any other type that implements speedy::Readable 
// and speedy::Writable will work just fine
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

## Collections

### SVec
[source code](./src/collections/vec_direct)

// TODO: API

### SHashMap
[source code](./src/collections/hash_map_indirect)

// TODO: API

### SHashSet
[source code](./src/collections/hash_set.rs)

// TODO: API

### SBinaryHeap
[source code](./src/collections/binary_heap_indirect)

// TODO: API

### SBTreeMap
[source code](./src/collections/btree_map.rs)

// TODO: API

### SBTreeSet
[source code](./src/collections/btree_set.rs)

// TODO: API

### SCertifiedBTreeMap
[source code](./src/collections/certified_btree_map.rs)

// TODO: API

## Benchmarks
These benchmarks are run on my machine against testing environment, where I emulate stable memory with a huge vector.
Performance difference in real canister should be less significant because of real stable memory.

### Vec
```
"Classic vec push" 1000000 iterations: 46 ms
"Stable vec push" 1000000 iterations: 212 ms (x4.6 slower)

"Classic vec search" 1000000 iterations: 102 ms
"Stable vec search" 1000000 iterations: 151 ms (x1.4 slower)

"Classic vec pop" 1000000 iterations: 48 ms
"Stable vec pop" 1000000 iterations: 148 ms (x3 slower)

"Classic vec insert" 100000 iterations: 1068 ms
"Stable vec insert" 100000 iterations: 3779 ms (x3.5 slower)

"Classic vec remove" 100000 iterations: 1183 ms
"Stable vec remove" 100000 iterations: 3739 ms (x3.1 slower)
```

### Log
```
"Classic vec push" 1000000 iterations: 50 ms 
"Stable vec push" 1000000 iterations: 248 ms (x5 slower)

"Classic vec search" 1000000 iterations: 63 ms
"Stable vec search" 1000000 iterations: 2372 ms 

"Classic vec pop" 1000000 iterations: 52 ms
"Stable vec pop" 1000000 iterations: 156 ms
```

### Binary heap
```
"Classic binary heap push" 1000000 iterations: 461 ms
"Stable binary heap push" 1000000 iterations: 11668 ms (x25 slower)

"Classic binary heap peek" 1000000 iterations: 62 ms
"Stable binary heap peek" 1000000 iterations: 144 ms (x2.3 slower)

"Classic binary heap pop" 1000000 iterations: 715 ms
"Stable binary heap pop" 1000000 iterations: 16524 ms (x23 slower)
```

### Hash map
```
"Classic hash map insert" 1000000 iterations: 1519 ms
"Stable hash map insert" 1000000 iterations: 2689 ms (x1.7 slower)

"Classic hash map search" 1000000 iterations: 748 ms
"Stable hash map search" 1000000 iterations: 1120 ms (x1.5 slower)

"Classic hash map remove" 1000000 iterations: 938 ms
"Stable hash map remove" 1000000 iterations: 2095 ms (x2.2 slower)
```

### Hash set
```
"Classic hash set insert" 1000000 iterations: 1214 ms
"Stable hash set insert" 1000000 iterations: 3210 ms (x2.6 slower)

"Classic hash set search" 1000000 iterations: 701 ms
"Stable hash set search" 1000000 iterations: 823 ms (x1.3 slower)

"Classic hash set remove" 1000000 iterations: 924 ms
"Stable hash set remove" 1000000 iterations: 1933 ms (x2.0 slower)
```

### BTree map
```
"Classic btree map insert" 1000000 iterations: 3413 ms
"Stable btree map insert" 1000000 iterations: 7848 ms (x2.3 slower)

"Classic btree map search" 1000000 iterations: 2053 ms
"Stable btree map search" 1000000 iterations: 7128 ms (x3.4 slower)

"Classic btree map remove" 1000000 iterations: 2216 ms
"Stable btree map remove" 1000000 iterations: 7986 ms (x3.6 slower)
```

### BTree set
```
"Classic btree set insert" 1000000 iterations: 3654 ms
"Stable btree set insert" 1000000 iterations: 9015 ms (x2.5 slower)

"Classic btree set search" 1000000 iterations: 2160 ms
"Stable btree set search" 1000000 iterations: 5111 ms (x2.3 slower)

"Classic btree set remove" 1000000 iterations: 2012 ms
"Stable btree set remove" 1000000 iterations: 7850 ms (x3.9 slower)
```

### Certified BTree map
```
"RBTree map insert" 10000 iterations: 10101 ms
"Stable certified btree map insert" 10000 iterations: 13798 ms (x1.3 slower)

"RBTree map search" 10000 iterations: 4 ms
"Stable certified btree map search" 10000 iterations: 34 ms (x8.5 slower)

"RBTree map witness" 10000 iterations: 4072 ms
"Stable certified btree map witness" 10000 iterations: 3184 ms (x1.2 faster)

"RBTree map remove" 10000 iterations: 12327 ms
"Stable certified btree map remove" 10000 iterations: 7915 ms (x1.5 faster)
```

## Performance counter canister
There is also a performance counter canister that I use to benchmark this library.
It can measure the amount of computations being performed during various operations over collections.

### Vec
```
› _a1_standard_vec_push(1000000) -> (59104497)
› _a2_stable_vec_push(1000000) -> (139668340) - x2.3 slower

› _b1_standard_vec_get(1000000) -> (28000204)
› _b2_stable_vec_get(1000000) -> (101000204) - x3.6 slower

› _c1_standard_vec_pop(1000000) -> (16000202)
› _c2_stable_vec_pop(1000000) -> (101000202) - x6.3 slower
```

### Binary heap
```
› _d1_standard_binary_heap_push(10000) -> (3950685)
› _d2_stable_binary_heap_push(10000) -> (47509416) - x12 slower

› _e1_standard_binary_heap_peek(10000) -> (180202)
› _e2_stable_binary_heap_peek(10000) -> (990202) - x5.5 slower

› _f1_standard_binary_heap_pop(10000) -> (5470367)
› _f2_stable_binary_heap_pop(10000) -> (68703887) - x12 slower
```

### Hash map
```
› _g1_standard_hash_map_insert(100000) -> (118009382)
› _g2_stable_hash_map_insert(100000) -> (296932746) - x2.5 slower

› _h1_standard_hash_map_get(100000) -> (46628530)
› _h2_stable_hash_map_get(100000) -> (75102338) - x1.6 slower

› _i1_standard_hash_map_remove(100000) -> (55432310)
› _i2_stable_hash_map_remove(100000) -> (82431271) - x1.4 slower
```

### Hash set
```
› _j1_standard_hash_set_insert(100000) -> (119107220)
› _j2_stable_hash_set_insert(100000) -> (280255730) - x2.3 slower

› _k1_standard_hash_set_contains(100000) -> (51403728)
› _k2_stable_hash_set_contains(100000) -> (67146485) - x1.3 slower

› _l1_standard_hash_set_remove(100000) -> (55424480)
› _l2_stable_hash_set_remove(100000) -> (81031271) - x1.4 slower
```

### BTree map
```
› _m1_standard_btree_map_insert(10000) -> (16868602)
› _m2_stable_btree_map_insert(10000) -> (399357425) - x23 slower

› _n1_standard_btree_map_get(10000) -> (7040037)
› _n2_stable_btree_map_get(10000) -> (101096721) - x14 slower

› _o1_standard_btree_map_remove(10000) -> (15155643)
› _o2_stable_btree_map_remove(10000) -> (333109461) - x21 slower
```

### BTree set
```
› _p1_standard_btree_set_insert(10000) -> (15914762)
› _p2_stable_btree_set_insert(10000) -> (495462730) - x31 slower

› _q1_standard_btree_set_contains(10000) -> (6830037)
› _q2_stable_btree_set_contains(10000) -> (99122577) - x14 slower

› _r1_standard_btree_set_remove(10000) -> (10650814)
› _r2_stable_btree_set_remove(10000) -> (317533303) - x29 slower
```

## Contribution
This is an emerging software, so any help is greatly appreciated.
Feel free to propose PR's, architecture tips, bug reports or any other feedback.

## Test coverage check
* `cargo install grcov`
* `rustup component add llvm-tools-preview`
* `./coverage.sh --test` (won't rebuild without `--test`)