// SPDX-License-Identifier: GPL-3.0-or-later
// GNU libstdc++ GCC 12.2 translation, copyright FSF 2007-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see runtime README.
//! Shared LP64 identity-hash table with source bucket and iteration history.
use super::*;
// Default load factor one; hashtable_c++0x.cc:102-122, hashtable-aux.cc:30-45.
const GROWTH: &[usize] = &[
    1, 13, 29, 59, 127, 257, 541, 1109, 2357, 5087, 10273, 20753, 42043, 85229, 172933,
];
const BEFORE_BEGIN: usize = usize::MAX;
#[derive(Debug)]
struct Node<V> {
    key: u64,
    value: Option<V>,
    next: Option<usize>,
}
#[derive(Debug)]
pub(super) struct IdentityTable<V> {
    head: Option<usize>,
    nodes: Vec<Node<V>>,
    free: Vec<usize>,
    buckets: Vec<Option<usize>>,
    growth: usize,
    len: usize,
}
impl<V> Default for IdentityTable<V> {
    fn default() -> Self {
        Self {
            head: None,
            nodes: Vec::new(),
            free: Vec::new(),
            buckets: Vec::new(),
            growth: 0,
            len: 0,
        }
    }
}
impl<V> IdentityTable<V> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn bucket_count(&self) -> usize {
        self.buckets.len().max(1)
    }
    pub fn bucket(&self, key: u64) -> usize {
        // Integral and pointer hashes preserve the value; functional_hash.h:107-112,154-173.
        (key % self.bucket_count() as u64) as usize
    }
    fn next(&self, previous: usize) -> Option<usize> {
        if previous == BEFORE_BEGIN {
            self.head
        } else {
            self.nodes[previous].next
        }
    }
    fn set_next(&mut self, previous: usize, next: Option<usize>) {
        if previous == BEFORE_BEGIN {
            self.head = next;
        } else {
            self.nodes[previous].next = next;
        }
    }
    fn insert_bucket_begin(&mut self, index: usize) {
        let bucket = self.bucket(self.nodes[index].key);
        if let Some(previous) = self.buckets[bucket] {
            self.nodes[index].next = self.next(previous);
            self.set_next(previous, Some(index));
        } else {
            self.nodes[index].next = self.head;
            self.head = Some(index);
            if let Some(next) = self.nodes[index].next {
                let former = self.bucket(self.nodes[next].key);
                self.buckets[former] = Some(index);
            }
            self.buckets[bucket] = Some(BEFORE_BEGIN);
        }
    }
    // Consume the old chain, reinserting at bucket heads; hashtable.h:2560-2591.
    fn rehash(&mut self) {
        self.growth += 1;
        self.buckets = vec![None; GROWTH[self.growth]];
        let mut current = self.head.take();
        while let Some(index) = current {
            current = self.nodes[index].next;
            self.insert_bucket_begin(index);
        }
    }
    fn find(&self, key: u64) -> Option<usize> {
        let bucket = self.bucket(key);
        let mut current = self
            .buckets
            .get(bucket)
            .copied()
            .flatten()
            .and_then(|p| self.next(p));
        while let Some(index) = current {
            let node = &self.nodes[index];
            if self.bucket(node.key) != bucket {
                break;
            }
            if node.key == key {
                return Some(index);
            }
            current = node.next;
        }
        None
    }
    pub fn get(&self, key: u64) -> Option<&V> {
        self.find(key).and_then(|i| self.nodes[i].value.as_ref())
    }
    pub fn get_mut(&mut self, key: u64) -> Option<&mut V> {
        let index = self.find(key)?;
        self.nodes[index].value.as_mut()
    }
    // Detect duplicates before growth; hashtable.h:2059-2083,2144-2164,2231-2256.
    pub fn insert(&mut self, key: u64, value: V, limit: usize) -> Result<bool> {
        if self.find(key).is_some() {
            return Ok(false);
        }
        require(
            self.len < limit && self.len < GROWTH[GROWTH.len() - 1],
            "execution.identity_table.member_limit",
        )?;
        if self.growth == 0 || self.len >= self.bucket_count() {
            self.rehash();
        }
        let node = Node {
            key,
            value: Some(value),
            next: None,
        };
        let index = if let Some(index) = self.free.pop() {
            self.nodes[index] = node;
            index
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        };
        self.insert_bucket_begin(index);
        self.len += 1;
        Ok(true)
    }
    // Repair both bucket predecessors before unlinking; hashtable.h:2016-2033,2311-2331.
    pub fn erase(&mut self, key: u64) -> Option<V> {
        let index = self.find(key)?;
        let bucket = self.bucket(key);
        let mut previous = self.buckets[bucket]?;
        while self.next(previous) != Some(index) {
            previous = self.next(previous)?;
        }
        let next = self.nodes[index].next;
        let next_bucket = next.map(|i| self.bucket(self.nodes[i].key));
        if self.buckets[bucket] == Some(previous) {
            if next_bucket != Some(bucket) {
                if let Some(next_bucket) = next_bucket {
                    self.buckets[next_bucket] = self.buckets[bucket];
                }
                self.buckets[bucket] = None;
            }
        } else if let Some(next_bucket) = next_bucket {
            if next_bucket != bucket {
                self.buckets[next_bucket] = Some(previous);
            }
        }
        self.set_next(previous, next);
        self.len -= 1;
        self.free.push(index);
        self.nodes[index].value.take()
    }
    pub fn first_key(&self) -> Option<u64> {
        self.head.map(|index| self.nodes[index].key)
    }
    pub fn next_key(&self, key: u64) -> Option<u64> {
        self.find(key)
            .and_then(|index| self.nodes[index].next)
            .map(|index| self.nodes[index].key)
    }
    pub fn iter(&self) -> impl Iterator<Item = (u64, &V)> {
        std::iter::successors(self.head, |&index| self.nodes[index].next).map(|index| {
            (
                self.nodes[index].key,
                self.nodes[index].value.as_ref().expect("linked node"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn erase_repairs_collision_heads_tails_and_reuses_slots_without_shrinking() {
        let mut table = IdentityTable::default();
        for key in [1, 14, 27, 2, 15, 3] {
            assert!(table.insert(key, key, 100).unwrap());
        }
        assert_eq!(
            table.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            [3, 15, 2, 27, 14, 1]
        );
        for key in [3, 27, 2, 1, 14, 15] {
            assert_eq!(table.erase(key), Some(key));
            assert!(table.get(key).is_none());
        }
        assert_eq!(table.len(), 0);
        assert_eq!(table.bucket_count(), 13);
        assert!(table.insert(0, 7, 100).unwrap());
        assert!(!table.insert(0, 9, 100).unwrap());
        assert_eq!(table.get(0), Some(&7));
        *table.get_mut(0).unwrap() = 8;
        assert_eq!(table.erase(0), Some(8));
        assert_eq!(table.nodes.len(), 6);
    }
}
