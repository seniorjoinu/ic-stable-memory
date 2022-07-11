use crate::collections::vec::SVec;
use crate::OutOfMemory;
use candid::{CandidType, Deserialize};
use serde::de::DeserializeOwned;
use std::cmp::Ordering;

#[derive(CandidType, Deserialize)]
pub enum SHeapType {
    Min,
    Max,
}

#[derive(CandidType, Deserialize)]
pub struct SBinaryHeap<T> {
    ty: SHeapType,
    arr: SVec<T>,
}

impl<T: CandidType + DeserializeOwned + Ord> SBinaryHeap<T> {
    pub fn new(ty: SHeapType) -> Self {
        Self {
            ty,
            arr: SVec::new(),
        }
    }

    pub fn new_with_capacity(ty: SHeapType, capacity: u64) -> Self {
        Self {
            ty,
            arr: SVec::new_with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, elem: &T) -> Result<(), OutOfMemory> {
        self.arr.push(elem)?;
        let len = self.len();
        if len == 1 {
            return Ok(());
        }

        let mut idx = len - 1;

        loop {
            let parent_idx = idx / 2;
            let parent = self.arr.get_cloned(parent_idx).unwrap();

            let mut flag = false;

            match self.ty {
                SHeapType::Min => {
                    if matches!(parent.cmp(elem), Ordering::Greater) {
                        flag = true;
                    }
                }
                SHeapType::Max => {
                    if matches!(parent.cmp(elem), Ordering::Less) {
                        flag = true;
                    }
                }
            };

            if flag {
                self.arr.swap(idx, parent_idx);
                idx = parent_idx;

                if idx > 0 {
                    continue;
                }
            }

            break;
        }

        Ok(())
    }

    pub fn peek(&self) -> Option<T> {
        self.arr.get_cloned(0)
    }

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();

        if len == 1 {
            return self.arr.pop();
        }

        self.arr.swap(0, len - 1);
        let elem = self.pop().unwrap();

        let last_idx = len - 2;

        let mut idx = 0;

        loop {
            let left_child_idx = idx * 2;
            let right_child_idx = idx * 2 + 1;

            if left_child_idx > last_idx {
                return Some(elem);
            }

            let left_child = self.arr.get_cloned(left_child_idx).unwrap();

            if right_child_idx > last_idx {
                let mut flag = false;

                match self.ty {
                    SHeapType::Min => {
                        if elem > left_child {
                            flag = true;
                        }
                    }
                    SHeapType::Max => {
                        if elem < left_child {
                            flag = true;
                        }
                    }
                };

                if flag {
                    self.arr.swap(idx, left_child_idx);
                    idx = left_child_idx;

                    // it will return the element at the next loop
                    continue;
                }
            }

            let right_child = self.arr.get_cloned(right_child_idx).unwrap();

            match self.ty {
                SHeapType::Min => {
                    if left_child <= right_child && left_child < elem {
                        self.arr.swap(idx, left_child_idx);
                        idx = left_child_idx;

                        continue;
                    }

                    if right_child <= left_child && right_child < elem {
                        self.arr.swap(idx, right_child_idx);
                        idx = right_child_idx;

                        continue;
                    }
                }
                SHeapType::Max => {
                    if left_child >= right_child && left_child > elem {
                        self.arr.swap(idx, left_child_idx);
                        idx = left_child_idx;

                        continue;
                    }

                    if right_child >= left_child && right_child > elem {
                        self.arr.swap(idx, right_child_idx);
                        idx = right_child_idx;

                        continue;
                    }
                }
            }
        }
    }

    pub fn len(&self) -> u64 {
        self.arr.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arr.is_empty()
    }
}
