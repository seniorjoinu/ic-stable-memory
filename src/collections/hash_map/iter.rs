use crate::collections::hash_map::SHashMap;
use copy_as_bytes::traits::{AsBytes, SuperSized};

pub struct SHashMapIter<'a, K, V> {
    map: &'a SHashMap<K, V>,
    offset: usize,
    max_offset: usize,
}

impl<'a, K: SuperSized, V: SuperSized> SHashMapIter<'a, K, V> {
    pub fn new(map: &'a SHashMap<K, V>) -> Self {
        let max_offset = map.capacity() * (1 + K::SIZE + V::SIZE);

        Self {
            max_offset,
            offset: 0,
            map,
        }
    }
}

impl<'a, K: AsBytes, V: AsBytes> Iterator for SHashMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let table = self.map.table?;

        loop {
            if self.offset == self.max_offset {
                break None;
            }

            let mut key_flag = [0u8];
            table.read_bytes(self.offset, &mut key_flag);

            match key_flag[0] {
                EMPTY => {
                    self.offset += 1 + K::SIZE + V::SIZE;
                    continue;
                }
                TOMBSTONE => {
                    self.offset += 1 + K::SIZE + V::SIZE;
                    continue;
                }
                OCCUPIED => {
                    let mut key_at_idx = K::super_size_u8_arr();
                    table.read_bytes(self.offset + 1, &mut key_at_idx);

                    let mut value_at_idx = V::super_size_u8_arr();
                    table.read_bytes(self.offset + 1 + K::SIZE, &mut value_at_idx);

                    self.offset += 1 + K::SIZE + V::SIZE;

                    break Some((K::from_bytes(key_at_idx), V::from_bytes(value_at_idx)));
                }
                _ => unreachable!(),
            }
        }
    }
}
