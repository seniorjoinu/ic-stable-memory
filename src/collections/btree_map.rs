use crate::primitive::s_slice::{SSlice, PTR_SIZE};
use crate::{allocate, OutOfMemory};

pub struct BTreeMap {
    root: SSlice<BTreeNode>,
    len: u64,
}

struct BTreeNode;

enum ChildKind {
    Left,
    Right,
}

impl SSlice<BTreeNode> {
    fn new_btree_node(m: usize) -> Result<Self, OutOfMemory> {
        let mut slice = allocate::<BTreeNode>(m * 2 * PTR_SIZE)?;
        slice.set_len(0);

        Ok(slice)
    }

    fn get_key_offset(&self, idx: usize) -> usize {
        assert!(idx < self.capacity(), "Out of bounds");

        (idx + 1) * PTR_SIZE
    }

    fn get_child_offset(&self, key_idx: usize, kind: ChildKind) -> usize {
        assert!(key_idx < self.capacity(), "Out of bounds");

        let mut child_idx = self.capacity() + key_idx;
        if matches!(kind, ChildKind::Right) {
            child_idx += 1;
        }

        (child_idx + 1) * PTR_SIZE
    }

    fn get_key_ptr(&self, idx: usize) -> u64 {
        self._read_word(self.get_key_offset(idx))
    }

    fn set_key_ptr(&mut self, idx: usize, ptr: u64) {
        self._write_word(self.get_key_offset(idx), ptr);
    }

    fn get_child_ptr(&self, key_idx: usize, kind: ChildKind) -> u64 {
        self._read_word(self.get_child_offset(key_idx, kind))
    }

    fn set_child_ptr(&self, key_idx: usize, kind: ChildKind, ptr: u64) {
        self._write_word(self.get_child_offset(key_idx, kind), ptr);
    }

    fn move_right(&self, key_idx: usize) {
        let old_keys_offset = self.get_key_offset(key_idx);
        let new_keys_offset = old_keys_offset + PTR_SIZE;
        let keys_size = (self.len() - key_idx) * PTR_SIZE;
        let mut keys_buf = vec![0u8; keys_size];

        self._read_bytes(old_keys_offset, &mut keys_buf);
        self._write_bytes(new_keys_offset, &keys_buf);

        let old_children_offset = self.get_child_offset(key_idx, ChildKind::Left);
        let new_children_offset = old_children_offset + PTR_SIZE;
        let children_size = (self.len() + 1 - key_idx) * PTR_SIZE;
        let mut children_buf = vec![0u8; children_size];

        self._read_bytes(old_children_offset, &mut children_buf);
        self._write_bytes(new_children_offset, &children_buf);
    }

    fn move_left(&self, key_idx: usize) {
        let len = self.len();

        if len == 1 {
            self._write_bytes(0, &vec![0u8; self.get_size_bytes()]);
            return;
        }

        let old_keys_offset = self.get_key_offset(key_idx);
        let new_keys_offset = old_keys_offset - PTR_SIZE;
        let keys_size = (len - key_idx) * PTR_SIZE;
        let mut keys_buf = vec![0u8; keys_size];

        self._read_bytes(old_keys_offset, &mut keys_buf);
        self._write_bytes(new_keys_offset, &keys_buf);

        let old_children_offset = self.get_child_offset(key_idx, ChildKind::Left);
        let new_children_offset = old_children_offset - PTR_SIZE;
        let children_size = (len + 1 - key_idx) * PTR_SIZE;
        let mut children_buf = vec![0u8; children_size];

        self._read_bytes(old_children_offset, &mut children_buf);
        self._write_bytes(new_children_offset, &children_buf);
    }

    fn set_len(&mut self, new_len: usize) {
        assert!(new_len <= self.capacity(), "Len out of bounds");

        self._write_word(0, new_len as u64);
    }

    fn insert(&mut self, idx: usize, key: u64) {
        assert!(!self.is_full(), "Full");
        let len = self.len();
        assert!(len >= idx, "No gaps allowed");

        if len > idx {
            self.move_right(idx);
        }

        self.set_key_ptr(idx, key);
        self.set_len(len + 1);
    }

    // removes the key and it's right child (returns all of them, just in case)
    fn remove(&mut self, idx: usize) -> (u64, u64, u64) {
        assert!(!self.is_empty(), "Empty");
        let len = self.len();
        assert!(len > idx, "Out of bounds");

        let key = self.get_key_ptr(idx);
        let left_child = self.get_child_ptr(idx, ChildKind::Left);
        let right_child = self.get_child_ptr(idx, ChildKind::Right);

        if idx < len - 1 {
            self.move_left(idx + 1);
            self.set_key_ptr(len - 1, 0);
            self.set_child_ptr(len - 1, ChildKind::Right, 0);
        }
        self.set_len(len - 1);

        (key, left_child, right_child)
    }

    fn len(&self) -> usize {
        self._read_word(0) as usize
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    fn capacity(&self) -> usize {
        self.get_size_bytes() / PTR_SIZE / 2 - 1
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::ChildKind;
    use crate::{init_allocator, stable, SSlice};

    #[test]
    fn nodes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut node = SSlice::new_btree_node(5).unwrap();
        node.insert(0, 10);
        node.set_child_ptr(0, ChildKind::Left, 0);
        node.set_child_ptr(0, ChildKind::Right, 1);

        node.insert(0, 20);
        node.set_child_ptr(0, ChildKind::Left, 2);

        node.insert(0, 30);
        node.set_child_ptr(0, ChildKind::Left, 3);

        node.insert(3, 40);
        node.set_child_ptr(3, ChildKind::Right, 4);

        assert_eq!(node.len(), 4);
        assert_eq!(node.capacity(), 4);

        assert_eq!(node.get_key_ptr(0), 30);
        assert_eq!(node.get_child_ptr(0, ChildKind::Left), 3);

        assert_eq!(node.get_key_ptr(1), 20);
        assert_eq!(node.get_child_ptr(1, ChildKind::Left), 2);

        assert_eq!(node.get_key_ptr(2), 10);
        assert_eq!(node.get_child_ptr(2, ChildKind::Left), 0);
        assert_eq!(node.get_child_ptr(2, ChildKind::Right), 1);

        assert_eq!(node.get_key_ptr(3), 40);
        assert_eq!(node.get_child_ptr(3, ChildKind::Right), 4);

        assert_eq!(node.remove(3), (40, 1, 4));
        assert_eq!(node.remove(0), (30, 3, 2));
        assert_eq!(node.remove(0), (20, 2, 0));
        assert_eq!(node.remove(0), (10, 0, 1));
    }
}
