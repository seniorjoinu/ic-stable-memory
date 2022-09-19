use ic_cdk::api::call::performance_counter;
use ic_cdk::api::time;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_memory::collections::binary_heap::{SBinaryHeap, SHeapType};
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
use std::hash::Hasher;

static mut STANDARD_VEC: Option<Vec<u64>> = None;
static mut STANDARD_BINARY_HEAP: Option<BinaryHeap<u64>> = None;
static mut STANDARD_HASHMAP: Option<HashMap<u64, u64>> = None;
static mut STANDARD_HASHSET: Option<HashSet<u64>> = None;
static mut STANDARD_BTREEMAP: Option<BTreeMap<u64, u64>> = None;
static mut STANDARD_BTREESET: Option<BTreeSet<u64>> = None;

type StableVec = SVec<u64>;
type StableBinaryHeap = SBinaryHeap<u64>;
type StableHashMap = SHashMap<u64, u64>;
type StableHashSet = SHashSet<u64>;
type StableBTreeMap = SBTreeMap<u64, u64>;
type StableBTreeSet = SBTreeSet<u64>;

static mut HASHER: Option<DefaultHasher> = None;

#[init]
fn init() {
    stable_memory_init(true, 0);

    s! { StableVec = SVec::new() };
    s! { StableBinaryHeap = SBinaryHeap::new(SHeapType::Max) };
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

fn get_random_u64(seed: u64) -> u64 {
    unsafe {
        let hasher = HASHER.as_mut().unwrap();
        hasher.write_u64(seed);

        hasher.finish()
    }
}

#[update]
fn _a1_standard_vec_push(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let vec = STANDARD_VEC.as_mut().unwrap();
        let seed = time();

        for _ in 0..count {
            vec.push(get_random_u64(seed));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _a2_stable_vec_push(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut vec = s!(StableVec);
    let seed = time();

    for _ in 0..count {
        vec.push(&get_random_u64(seed));
    }

    s! { StableVec = vec };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _b1_standard_vec_get(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let vec = STANDARD_VEC.as_mut().unwrap();

        for _ in 0..count {
            let idx = (get_random_u64(time()) as usize) % vec.len();
            vec.get(idx).unwrap();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _b2_stable_vec_get(count: u32) -> u64 {
    let before = performance_counter(0);

    let vec = s!(StableVec);

    for _ in 0..count {
        let idx = get_random_u64(time()) % vec.len();
        vec.get_cloned(idx).unwrap();
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _c1_standard_vec_pop(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let vec = STANDARD_VEC.as_mut().unwrap();

        for _ in 0..count {
            vec.pop();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _c2_stable_vec_pop(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut vec = s!(StableVec);

    for _ in 0..count {
        vec.pop();
    }

    s! { StableVec = vec };

    let after = performance_counter(0);

    after - before
}

#[update]
fn _d1_standard_binary_heap_push(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let binary_heap = STANDARD_BINARY_HEAP.as_mut().unwrap();
        let seed = time();

        for _ in 0..count {
            binary_heap.push(get_random_u64(seed));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _d2_stable_binary_heap_push(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut binary_heap = s!(StableBinaryHeap);

    for _ in 0..count {
        binary_heap.push(&get_random_u64(time()));
    }

    s! { StableBinaryHeap = binary_heap };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _e1_standard_binary_heap_peek(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let binary_heap = STANDARD_BINARY_HEAP.as_mut().unwrap();

        for _ in 0..count {
            binary_heap.peek().unwrap();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _e2_stable_binary_heap_peek(count: u32) -> u64 {
    let before = performance_counter(0);

    let binary_heap = s!(StableBinaryHeap);

    for _ in 0..count {
        binary_heap.peek().unwrap();
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _f1_standard_binary_heap_pop(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let binary_heap = STANDARD_BINARY_HEAP.as_mut().unwrap();

        for _ in 0..count {
            binary_heap.pop();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _f2_stable_binary_heap_pop(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut binary_heap = s!(StableBinaryHeap);

    for _ in 0..count {
        binary_heap.pop();
    }

    s! { StableBinaryHeap = binary_heap };

    let after = performance_counter(0);

    after - before
}

#[update]
fn _g1_standard_hash_map_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_map = STANDARD_HASHMAP.as_mut().unwrap();

        for key in 0..count {
            hash_map.insert(key as u64, 1);
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _g2_stable_hash_map_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut hash_map = s!(StableHashMap);

    for key in 0..count {
        hash_map.insert(key as u64, &1);
    }

    s! { StableHashMap = hash_map };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _h1_standard_hash_map_get(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_map = STANDARD_HASHMAP.as_mut().unwrap();

        for key in 0..count {
            hash_map.get(&(key as u64)).unwrap();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _h2_stable_hash_map_get(count: u32) -> u64 {
    let before = performance_counter(0);

    let hash_map = s!(StableHashMap);

    for key in 0..count {
        hash_map.get_cloned(&(key as u64)).unwrap();
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _i1_standard_hash_map_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_map = STANDARD_HASHMAP.as_mut().unwrap();

        for key in 0..count {
            hash_map.remove(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _i2_stable_hash_map_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut hash_map = s!(StableHashMap);

    for key in 0..count {
        hash_map.remove(&(key as u64));
    }

    s! { StableHashMap = hash_map };

    let after = performance_counter(0);

    after - before
}

#[update]
fn _j1_standard_hash_set_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_set = STANDARD_HASHSET.as_mut().unwrap();

        for key in 0..count {
            hash_set.insert(key as u64);
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _j2_stable_hash_set_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut hash_set = s!(StableHashSet);

    for key in 0..count {
        hash_set.insert(key as u64);
    }

    s! { StableHashSet = hash_set };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _k1_standard_hash_set_contains(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_set = STANDARD_HASHSET.as_mut().unwrap();

        for key in 0..count {
            hash_set.contains(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _k2_stable_hash_set_contains(count: u32) -> u64 {
    let before = performance_counter(0);

    let hash_set = s!(StableHashSet);

    for key in 0..count {
        hash_set.contains(&(key as u64));
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _l1_standard_hash_set_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let hash_set = STANDARD_HASHSET.as_mut().unwrap();

        for key in 0..count {
            hash_set.remove(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _l2_stable_hash_set_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut hash_set = s!(StableHashSet);

    for key in 0..count {
        hash_set.remove(&(key as u64));
    }

    s! { StableHashSet = hash_set };

    let after = performance_counter(0);

    after - before
}

#[update]
fn _m1_standard_btree_map_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_map = STANDARD_BTREEMAP.as_mut().unwrap();

        for key in 0..count {
            btree_map.insert(key as u64, 1);
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _m2_stable_btree_map_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut btree_map = s!(StableBTreeMap);

    for key in 0..count {
        btree_map.insert(key as u64, &1);
    }

    s! { StableBTreeMap = btree_map };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _n1_standard_btree_map_get(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_map = STANDARD_BTREEMAP.as_mut().unwrap();

        for key in 0..count {
            btree_map.get(&(key as u64)).unwrap();
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _n2_stable_btree_map_get(count: u32) -> u64 {
    let before = performance_counter(0);

    let btree_map = s!(StableBTreeMap);

    for key in 0..count {
        btree_map.get_cloned(&(key as u64));
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _o1_standard_btree_map_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_map = STANDARD_BTREEMAP.as_mut().unwrap();

        for key in 0..count {
            btree_map.remove(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _o2_stable_btree_map_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut btree_map = s!(StableBTreeMap);

    for key in 0..count {
        btree_map.remove(&(key as u64));
    }

    s! { StableBTreeMap = btree_map };

    let after = performance_counter(0);

    after - before
}

#[update]
fn _p1_standard_btree_set_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_set = STANDARD_BTREESET.as_mut().unwrap();

        for key in 0..count {
            btree_set.insert(key as u64);
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _p2_stable_btree_set_insert(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut btree_set = s!(StableBTreeSet);

    for key in 0..count {
        btree_set.insert(key as u64);
    }

    s! { StableBTreeSet = btree_set };

    let after = performance_counter(0);

    after - before
}

#[query]
fn _q1_standard_btree_set_contains(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_set = STANDARD_BTREESET.as_mut().unwrap();

        for key in 0..count {
            btree_set.contains(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[query]
fn _q2_stable_btree_set_contains(count: u32) -> u64 {
    let before = performance_counter(0);

    let btree_set = s!(StableBTreeSet);

    for key in 0..count {
        btree_set.contains(&(key as u64));
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _r1_standard_btree_set_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    unsafe {
        let btree_set = STANDARD_BTREESET.as_mut().unwrap();

        for key in 0..count {
            btree_set.remove(&(key as u64));
        }
    }

    let after = performance_counter(0);

    after - before
}

#[update]
fn _r2_stable_btree_set_remove(count: u32) -> u64 {
    let before = performance_counter(0);

    let mut btree_set = s!(StableBTreeSet);

    for key in 0..count {
        btree_set.remove(&(key as u64));
    }

    s! { StableBTreeSet = btree_set };

    let after = performance_counter(0);

    after - before
}
