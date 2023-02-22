![test coverage 88.82%](https://badgen.net/badge/coverage/88.82%25/green)


# IC Stable Memory

Allows using canister's stable memory as main memory.

## Installation
```toml
# cargo.toml

[dependencies]
ic-stable-memory = "0.4"
```

## Documentation
1. [Complete API documentation]

## Quick example
Check out [the example project](./examples/token) to find out more.

Let's imagine 

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