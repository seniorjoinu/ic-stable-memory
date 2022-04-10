use crate::mem::membox::common::{Side, PTR_SIZE};
use crate::{allocate, deallocate, MemBox};
use candid::types::{Serializer, Type};
use candid::{encode_one, CandidType, Error as CandidError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::marker::PhantomData;

const DEFAULT_STABLE_BTREE_CLASS: u16 = 16;

#[derive(Debug)]
pub enum StableBTreeMapError {
    CandidError(CandidError),
    OutOfMemory,
    NoBTreeMapAt,
}

struct BTreeNode<K, V> {
    key: PhantomData<K>,
    value: PhantomData<V>,
}

#[derive(CandidType, Deserialize)]
struct BTreeInfo<K, V> {
    class: u16,
    head: MemBox<BTreeNode<K, V>>,
}

pub struct StableBTreeMap<K, V> {
    membox: MemBox<BTreeInfo<K, V>>,
    key: PhantomData<K>,
    value: PhantomData<V>,
}

impl<K, V> Clone for StableBTreeMap<K, V>
where
    K: CandidType + DeserializeOwned + Ord,
    V: CandidType + DeserializeOwned,
{
    fn clone(&self) -> Self {
        Self {
            membox: self.membox.clone(),
            key: PhantomData::default(),
            value: PhantomData::default(),
        }
    }
}

impl<K, V> StableBTreeMap<K, V>
where
    K: CandidType + DeserializeOwned + Ord,
    V: CandidType + DeserializeOwned,
{
    pub fn new() -> Result<Self, StableBTreeMapError> {
        Self::new_with_class(DEFAULT_STABLE_BTREE_CLASS)
    }

    pub fn new_with_class(class: u16) -> Result<Self, StableBTreeMapError> {
        assert!(class > 1);

        let head = create_node_of_class::<K, V>(class)?;
        let info = BTreeInfo {
            class,
            head: head.clone(),
        };

        let encoded_info = encode_one(info)
            .map_err(|e| StableBTreeMapError::CandidError(e))
            .unwrap();
        let info_membox_res = allocate(encoded_info.len());
        if let Err(e) = info_membox_res {
            deallocate(head);

            return Err(StableBTreeMapError::OutOfMemory);
        }

        let mut info_membox = info_membox_res.unwrap();
        info_membox.set_encoded(encoded_info);

        Ok(Self {
            membox: info_membox,
            key: PhantomData::default(),
            value: PhantomData::default(),
        })
    }

    pub fn from_ptr(ptr: u64) -> Result<Self, StableBTreeMapError> {
        let membox = unsafe {
            MemBox::<BTreeInfo<K, V>>::from_ptr(ptr, Side::Start)
                .ok_or(StableBTreeMapError::NoBTreeMapAt)?
        };
        membox
            .get_cloned()
            .map_err(|e| StableBTreeMapError::CandidError(e.unwrap_candid()))?;

        Ok(Self {
            membox,
            key: PhantomData::default(),
            value: PhantomData::default(),
        })
    }
}

fn create_node_of_class<K, V>(class: u16) -> Result<MemBox<BTreeNode<K, V>>, StableBTreeMapError> {
    let node_size = (class as usize * 2 - 1) * PTR_SIZE;
    let mut membox =
        allocate::<BTreeNode<K, V>>(node_size).map_err(|_| StableBTreeMapError::OutOfMemory)?;

    let white = vec![0u8; node_size];
    membox._write_bytes(0, &white);

    Ok(membox)
}

impl<K, V> MemBox<BTreeNode<K, V>> {
    pub(crate) fn get_value_ptr(&self, idx: u16, class: u16) -> u64 {
        assert!(idx < class - 1);
        self._read_word(idx as usize * PTR_SIZE)
    }

    pub(crate) fn set_value_ptr(&mut self, idx: u16, value_ptr: u64, class: u16) {
        assert!(idx < class - 1);
        self._write_word(idx as usize * PTR_SIZE, value_ptr);
    }

    pub(crate) fn get_child_ptr(&self, idx: u16, class: u16) -> u64 {
        assert!(idx < class);
        self._read_word((class - 1 + idx) as usize * PTR_SIZE)
    }

    pub(crate) fn set_child_ptr(&mut self, idx: u16, child_ptr: u64, class: u16) {
        assert!(idx < class);
        self._write_word((class - 1 + idx) as usize * PTR_SIZE, child_ptr);
    }
}
