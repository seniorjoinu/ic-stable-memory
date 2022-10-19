#![feature(generic_const_exprs)]

use ic_cdk::api::call::performance_counter;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::binary_heap::SBinaryHeap;
use ic_stable_memory::collections::btree_map::SBTreeMap;
use ic_stable_memory::collections::btree_set::SBTreeSet;
use ic_stable_memory::collections::hash_map::SHashMap;
use ic_stable_memory::collections::hash_set::SHashSet;
use ic_stable_memory::collections::vec::SVec;
use ic_stable_memory::{
    s, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::hint::black_box;

static mut STANDARD_VEC: Option<Vec<u64>> = None;
static mut STANDARD_BINARY_HEAP: Option<BinaryHeap<u64>> = None;
static mut STANDARD_HASHMAP: Option<HashMap<u64, u64>> = None;
static mut STANDARD_HASHSET: Option<HashSet<u64>> = None;
static mut STANDARD_BTREEMAP: Option<BTreeMap<u64, u64>> = None;
static mut STANDARD_BTREESET: Option<BTreeSet<u64>> = None;

type StableVec = SVec<u64, u64>;
type StableBinaryHeap = SBinaryHeap<u64, u64>;
type StableHashMap = SHashMap<u64, u64, u64, u64>;
type StableHashSet = SHashSet<u64, u64>;
type StableBTreeMap = SBTreeMap<u64, u64, u64, u64>;
type StableBTreeSet = SBTreeSet<u64, u64>;

static mut HASHER: Option<DefaultHasher> = None;

#[init]
fn init() {
    stable_memory_init(true, 0);

    s! { StableVec = SVec::new() };
    s! { StableBinaryHeap = SBinaryHeap::new() };
    s! { StableHashMap = SHashMap::new() };
    s! { StableHashSet = SHashSet::new() };
    s! { StableBTreeMap = SBTreeMap::new() };
    s! { StableBTreeSet = SBTreeSet::new() };

    unsafe {
        STANDARD_VEC = Some(Vec::new());
        STANDARD_BINARY_HEAP = Some(BinaryHeap::new());
        STANDARD_HASHMAP = Some(HashMap::new());
        STANDARD_HASHSET = Some(HashSet::new());
        STANDARD_BTREEMAP = Some(BTreeMap::new());
        STANDARD_BTREESET = Some(BTreeSet::new());

        HASHER = Some(DefaultHasher::default());
    }
}

#[pre_upgrade]
fn pre_upgrade() {
    stable_memory_pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade(0);

    unsafe {
        STANDARD_VEC = Some(Vec::new());
        STANDARD_BINARY_HEAP = Some(BinaryHeap::new());
        STANDARD_HASHMAP = Some(HashMap::new());
        STANDARD_HASHSET = Some(HashSet::new());
        STANDARD_BTREEMAP = Some(BTreeMap::new());
        STANDARD_BTREESET = Some(BTreeSet::new());

        HASHER = Some(DefaultHasher::default());
    }
}

#[update]
fn _a1_standard_vec_push(count: u32) -> u64 {
    let vec = unsafe { STANDARD_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count {
            vec.push(10);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _a2_stable_vec_push(count: u32) -> u64 {
    let mut vec = s!(StableVec);

    let res = {
        let before = performance_counter(0);

        for i in 0..count as u64 {
            vec.push(i);
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableVec = vec };

    res
}

#[query]
fn _b1_standard_vec_get(count: u32) -> u64 {
    let vec = unsafe { STANDARD_VEC.as_ref().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count as usize {
            let j = *vec.get(i).unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _b2_stable_vec_get(count: u32) -> u64 {
    let vec = s!(StableVec);

    {
        let before = performance_counter(0);

        for i in 0..count as usize {
            vec.get_copy(i).unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _c1_standard_vec_pop(count: u32) -> u64 {
    let vec = unsafe { STANDARD_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.pop();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _c2_stable_vec_pop(count: u32) -> u64 {
    let mut vec = s!(StableVec);

    let res = {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.pop();
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableVec = vec };

    res
}

#[update]
fn _d1_standard_binary_heap_push(count: u32) -> u64 {
    let binary_heap = unsafe { STANDARD_BINARY_HEAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count as u64 {
            binary_heap.push(black_box(i));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _d2_stable_binary_heap_push(count: u32) -> u64 {
    let mut binary_heap = s!(StableBinaryHeap);

    let res = {
        let before = performance_counter(0);

        for i in 0..count as u64 {
            binary_heap.push(black_box(i));
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBinaryHeap = binary_heap };

    res
}

#[query]
fn _e1_standard_binary_heap_peek(count: u32) -> u64 {
    let binary_heap = unsafe { STANDARD_BINARY_HEAP.as_ref().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            binary_heap.peek().unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _e2_stable_binary_heap_peek(count: u32) -> u64 {
    let binary_heap = s!(StableBinaryHeap);

    {
        let before = performance_counter(0);

        for _ in 0..count {
            binary_heap.peek().unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _f1_standard_binary_heap_pop(count: u32) -> u64 {
    let binary_heap = unsafe { STANDARD_BINARY_HEAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            binary_heap.pop();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _f2_stable_binary_heap_pop(count: u32) -> u64 {
    let mut binary_heap = s!(StableBinaryHeap);

    let res = {
        let before = performance_counter(0);

        for _ in 0..count {
            binary_heap.pop();
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBinaryHeap = binary_heap };

    res
}

#[update]
fn _g1_standard_hash_map_insert(count: u32) -> u64 {
    let hash_map = unsafe { STANDARD_HASHMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.insert(key as u64, 1);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _g2_stable_hash_map_insert(count: u32) -> u64 {
    let mut hash_map = s!(StableHashMap);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.insert(key as u64, 1);
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableHashMap = hash_map };

    res
}

#[query]
fn _h1_standard_hash_map_get(count: u32) -> u64 {
    let hash_map = unsafe { STANDARD_HASHMAP.as_ref().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.get(&(key as u64)).unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _h2_stable_hash_map_get(count: u32) -> u64 {
    let hash_map = s!(StableHashMap);

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.get_copy(&(key as u64)).unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _i1_standard_hash_map_remove(count: u32) -> u64 {
    let hash_map = unsafe { STANDARD_HASHMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _i2_stable_hash_map_remove(count: u32) -> u64 {
    let mut hash_map = s!(StableHashMap);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableHashMap = hash_map };

    res
}

#[update]
fn _j1_standard_hash_set_insert(count: u32) -> u64 {
    let hash_set = unsafe { STANDARD_HASHSET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.insert(key as u64);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _j2_stable_hash_set_insert(count: u32) -> u64 {
    let mut hash_set = s!(StableHashSet);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.insert(key as u64);
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableHashSet = hash_set };

    res
}

#[query]
fn _k1_standard_hash_set_contains(count: u32) -> u64 {
    let hash_set = unsafe { STANDARD_HASHSET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.contains(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _k2_stable_hash_set_contains(count: u32) -> u64 {
    let hash_set = s!(StableHashSet);

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.contains(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _l1_standard_hash_set_remove(count: u32) -> u64 {
    let hash_set = unsafe { STANDARD_HASHSET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _l2_stable_hash_set_remove(count: u32) -> u64 {
    let mut hash_set = s!(StableHashSet);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableHashSet = hash_set };

    res
}

#[update]
fn _m1_standard_btree_map_insert(count: u32) -> u64 {
    let btree_map = unsafe { STANDARD_BTREEMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.insert(key as u64, 1);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _m2_stable_btree_map_insert(count: u32) -> u64 {
    let mut btree_map = s!(StableBTreeMap);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.insert(key as u64, 1);
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBTreeMap = btree_map };

    res
}

#[query]
fn _n1_standard_btree_map_get(count: u32) -> u64 {
    let btree_map = unsafe { STANDARD_BTREEMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.get(&(key as u64)).unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _n2_stable_btree_map_get(count: u32) -> u64 {
    let btree_map = s!(StableBTreeMap);

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.get_copy(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _o1_standard_btree_map_remove(count: u32) -> u64 {
    let btree_map = unsafe { STANDARD_BTREEMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _o2_stable_btree_map_remove(count: u32) -> u64 {
    let mut btree_map = s!(StableBTreeMap);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBTreeMap = btree_map };

    res
}

#[update]
fn _p1_standard_btree_set_insert(count: u32) -> u64 {
    let btree_set = unsafe { STANDARD_BTREESET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.insert(key as u64);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _p2_stable_btree_set_insert(count: u32) -> u64 {
    let mut btree_set = s!(StableBTreeSet);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.insert(key as u64);
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBTreeSet = btree_set };

    res
}

#[query]
fn _q1_standard_btree_set_contains(count: u32) -> u64 {
    let btree_set = unsafe { STANDARD_BTREESET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.contains(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[query]
fn _q2_stable_btree_set_contains(count: u32) -> u64 {
    let btree_set = s!(StableBTreeSet);

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.contains(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _r1_standard_btree_set_remove(count: u32) -> u64 {
    let btree_set = unsafe { STANDARD_BTREESET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _r2_stable_btree_set_remove(count: u32) -> u64 {
    let mut btree_set = s!(StableBTreeSet);

    let res = {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.remove(&(key as u64));
        }

        let after = performance_counter(0);

        after - before
    };

    s! { StableBTreeSet = btree_set };

    res
}
