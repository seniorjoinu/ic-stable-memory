use ic_cdk::api::call::performance_counter;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_certified_map::{AsHashTree as AsRBHashTree, HashTree as RBHashTree, RbTree};
use ic_stable_memory::collections::{
    SBTreeMap, SBTreeSet, SCertifiedBTreeMap, SHashMap, SHashSet, SLog, SVec,
};
use ic_stable_memory::derive::{AsFixedSizeBytes, StableType};
use ic_stable_memory::utils::certification::{Hash, HashTree};
use ic_stable_memory::utils::DebuglessUnwrap;
use ic_stable_memory::{
    leaf, leaf_hash, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
    AsHashTree, AsHashableBytes,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(StableType, AsFixedSizeBytes, Ord, PartialOrd, Eq, PartialEq)]
struct WrappedNumber(u64);

impl AsHashTree for WrappedNumber {
    fn hash_tree(&self) -> HashTree {
        leaf(self.0.to_le_bytes().to_vec())
    }

    fn root_hash(&self) -> Hash {
        leaf_hash(&self.0.to_le_bytes())
    }
}

impl AsHashableBytes for WrappedNumber {
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }
}

impl AsRBHashTree for WrappedNumber {
    fn root_hash(&self) -> ic_certified_map::Hash {
        leaf_hash(&self.0.to_le_bytes())
    }

    fn as_hash_tree(&self) -> RBHashTree<'_> {
        RBHashTree::Leaf(Cow::Owned(self.0.to_le_bytes().to_vec()))
    }
}

static mut STANDARD_VEC: Option<Vec<u64>> = None;
static mut STANDARD_HASHMAP: Option<HashMap<u64, u64>> = None;
static mut STANDARD_HASHSET: Option<HashSet<u64>> = None;
static mut STANDARD_BTREEMAP: Option<BTreeMap<u64, u64>> = None;
static mut STANDARD_BTREESET: Option<BTreeSet<u64>> = None;
static mut STANDARD_CERTIFIED_MAP: Option<RbTree<[u8; 8], WrappedNumber>> = None;

static mut STABLE_VEC: Option<SVec<u64>> = None;
static mut STABLE_LOG: Option<SLog<u64>> = None;
static mut STABLE_HASHMAP: Option<SHashMap<u64, u64>> = None;
static mut STABLE_HASHSET: Option<SHashSet<u64>> = None;
static mut STABLE_BTREEMAP: Option<SBTreeMap<u64, u64>> = None;
static mut STABLE_BTREESET: Option<SBTreeSet<u64>> = None;
static mut STABLE_CERTIFIED_MAP: Option<SCertifiedBTreeMap<WrappedNumber, WrappedNumber>> = None;

#[init]
fn init() {
    stable_memory_init();

    unsafe {
        STANDARD_VEC = Some(Vec::new());
        STANDARD_HASHMAP = Some(HashMap::new());
        STANDARD_HASHSET = Some(HashSet::new());
        STANDARD_BTREEMAP = Some(BTreeMap::new());
        STANDARD_BTREESET = Some(BTreeSet::new());
        STANDARD_CERTIFIED_MAP = Some(RbTree::new());

        STABLE_VEC = Some(SVec::new());
        STABLE_LOG = Some(SLog::new());
        STABLE_HASHMAP = Some(SHashMap::new());
        STABLE_HASHSET = Some(SHashSet::new());
        STABLE_BTREEMAP = Some(SBTreeMap::new());
        STABLE_BTREESET = Some(SBTreeSet::new());
        STABLE_CERTIFIED_MAP = Some(SCertifiedBTreeMap::new());
    }
}

#[pre_upgrade]
fn pre_upgrade() {
    stable_memory_pre_upgrade().expect("Out of memory");
}

#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade();

    unsafe {
        STANDARD_VEC = Some(Vec::new());
        STANDARD_HASHMAP = Some(HashMap::new());
        STANDARD_HASHSET = Some(HashSet::new());
        STANDARD_BTREEMAP = Some(BTreeMap::new());
        STANDARD_BTREESET = Some(BTreeSet::new());
        STANDARD_CERTIFIED_MAP = Some(RbTree::new());

        STABLE_VEC = Some(SVec::new());
        STABLE_LOG = Some(SLog::new());
        STABLE_HASHMAP = Some(SHashMap::new());
        STABLE_HASHSET = Some(SHashSet::new());
        STABLE_BTREEMAP = Some(SBTreeMap::new());
        STABLE_BTREESET = Some(SBTreeSet::new());
        STABLE_CERTIFIED_MAP = Some(SCertifiedBTreeMap::new());
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
    let vec = unsafe { STABLE_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count {
            vec.push(10).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _a3_stable_log_push(count: u32) -> u64 {
    let log = unsafe { STABLE_LOG.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count {
            log.push(10).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
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
    let vec = unsafe { STABLE_VEC.as_ref().unwrap() };

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
fn _b3_stable_log_get(count: u32) -> u64 {
    let vec = unsafe { STABLE_LOG.as_ref().unwrap() };

    {
        let before = performance_counter(0);

        for i in 0..count as u64 {
            let j = *vec.get(i).unwrap();
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
    let vec = unsafe { STABLE_VEC.as_mut().unwrap() };

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
fn _c3_stable_log_pop(count: u32) -> u64 {
    let vec = unsafe { STABLE_LOG.as_mut().unwrap() };

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
fn _d1_standard_vec_insert(count: u32) -> u64 {
    let vec = unsafe { STANDARD_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.insert(0, 10);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _d2_stable_vec_insert(count: u32) -> u64 {
    let vec = unsafe { STABLE_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.insert(0, 10).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _e1_standard_vec_remove(count: u32) -> u64 {
    let vec = unsafe { STANDARD_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.remove(0);
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _e2_stable_vec_remove(count: u32) -> u64 {
    let vec = unsafe { STABLE_VEC.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for _ in 0..count {
            vec.remove(0);
        }

        let after = performance_counter(0);

        after - before
    }
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
    let hash_map = unsafe { STABLE_HASHMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.insert(key as u64, 1).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
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
    let hash_map = unsafe { STABLE_HASHMAP.as_ref().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_map.get(&(key as u64)).unwrap();
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
    let hash_map = unsafe { STABLE_HASHMAP.as_mut().unwrap() };

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
    let hash_set = unsafe { STABLE_HASHSET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            hash_set.insert(key as u64).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
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
    let hash_set = unsafe { STABLE_HASHSET.as_mut().unwrap() };

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
    let hash_set = unsafe { STABLE_HASHSET.as_mut().unwrap() };

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
    let btree_map = unsafe { STABLE_BTREEMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.insert(key as u64, 1).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
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
    let btree_map = unsafe { STABLE_BTREEMAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_map.get(&(key as u64)).unwrap();
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
    let btree_map = unsafe { STABLE_BTREEMAP.as_mut().unwrap() };

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
    let btree_set = unsafe { STABLE_BTREESET.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count {
            btree_set.insert(key as u64).debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
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
    let btree_set = unsafe { STABLE_BTREESET.as_mut().unwrap() };

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
    let btree_set = unsafe { STABLE_BTREESET.as_mut().unwrap() };

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
fn _s1_standard_certified_map_insert(count: u32) -> u64 {
    let certified_map = unsafe { STANDARD_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.insert(key.to_le_bytes(), WrappedNumber(key));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _s2_stable_certified_map_insert(count: u32) -> u64 {
    let certified_map = unsafe { STABLE_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map
                .insert_and_commit(WrappedNumber(key), WrappedNumber(key))
                .debugless_unwrap();
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _s3_stable_certified_map_insert_batch(count: u32, batch_size: u32) -> u64 {
    let certified_map = unsafe { STABLE_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map
                .insert(WrappedNumber(key), WrappedNumber(key))
                .debugless_unwrap();

            if key as u32 % batch_size == 0 {
                certified_map.commit();
            }
        }

        certified_map.commit();

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _t1_standard_certified_map_witness(count: u32) -> u64 {
    let certified_map = unsafe { STANDARD_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.witness(&key.to_le_bytes());
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _t2_stable_certified_map_witness(count: u32) -> u64 {
    let certified_map = unsafe { STABLE_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.witness(&WrappedNumber(key));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _u1_standard_certified_map_remove(count: u32) -> u64 {
    let certified_map = unsafe { STANDARD_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.delete(&key.to_le_bytes());
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _u2_stable_certified_map_remove(count: u32) -> u64 {
    let certified_map = unsafe { STABLE_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.remove_and_commit(&WrappedNumber(key));
        }

        let after = performance_counter(0);

        after - before
    }
}

#[update]
fn _u3_stable_certified_map_remove_batch(count: u32, batch_size: u32) -> u64 {
    let certified_map = unsafe { STABLE_CERTIFIED_MAP.as_mut().unwrap() };

    {
        let before = performance_counter(0);

        for key in 0..count as u64 {
            certified_map.remove(&WrappedNumber(key));

            if key as u32 % batch_size == 0 {
                certified_map.commit();
            }
        }

        certified_map.commit();

        let after = performance_counter(0);

        after - before
    }
}
