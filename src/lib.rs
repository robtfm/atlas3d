#![feature(let_else)]
use glam::UVec3;
use std::{collections::HashMap, hash::Hash};

#[cfg(test)]
mod tests {
    use crate::{AtlasPage, Slot};
    use glam::UVec3;

    #[test]
    fn insert_stuff() {
        let mut page = AtlasPage::new(UVec3::splat(10));

        let h0 = 0;
        let h1 = 1;
        let h2 = 2;

        // first item at 0
        assert_eq!(page.insert(h0, UVec3::splat(6)), Slot::New(UVec3::ZERO));

        // inserting again gets same
        assert_eq!(
            page.insert(h0, UVec3::splat(6)),
            Slot::Existing(UVec3::ZERO)
        );

        // second item doesn't fit
        assert_eq!(page.insert(h1, UVec3::splat(6)), Slot::NoFit);

        // smaller item fits right
        assert_eq!(
            page.insert(h2, UVec3::splat(4)),
            Slot::New(UVec3::new(6, 0, 0))
        );

        // second item fits after removal
        page.remove(&h0);
        assert_eq!(page.insert(h1, UVec3::splat(6)), Slot::New(UVec3::ZERO));

        // first item no longer fits
        assert_eq!(page.insert(h0, UVec3::splat(6)), Slot::NoFit);

        let mut page = AtlasPage::new(UVec3::splat(10));
        page.insert(h0, UVec3::splat(2));
        let Slot::New(pos) = page.insert(h1, UVec3::splat(2)) else {panic!()};
        page.insert(h2, UVec3::splat(2));
        page.remove(&h1);
        // reinsert gets original location if not paged out
        assert_eq!(page.insert(h1, UVec3::splat(2)), Slot::Existing(pos))
    }

    #[test]
    fn check_lru() {
        let mut page = AtlasPage::new(UVec3::splat(10));

        let h0 = 0;
        let h1 = 1;

        page.insert(h1, UVec3::ONE);
        page.remove(&h1);

        assert_eq!(page.insert(h0, UVec3::ONE), Slot::New(UVec3::X));
        assert_eq!(page.insert(h1, UVec3::ONE), Slot::Existing(UVec3::ZERO));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AtlasHandle(usize);

#[derive(Clone, Copy, Debug)]
pub struct AtlasInfo {
    pub size: UVec3,
    pub position: UVec3,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Slot {
    NoFit,
    New(UVec3),
    Existing(UVec3),
}

#[derive(Clone)]
pub struct AtlasPage<H: Eq + Hash + Clone> {
    pub dim: UVec3,
    live_items: HashMap<H, AtlasInfo>,
    dead_items: HashMap<H, AtlasInfo>,
}

impl<H: Eq + Hash + Clone> AtlasPage<H> {
    pub fn new(dim: UVec3) -> Self {
        Self {
            dim,
            live_items: Default::default(),
            dead_items: Default::default(),
        }
    }

    fn measure(&self, pos: UVec3, size: UVec3) -> Option<(u32, Vec<H>)> {
        // check if we fit within the page
        if (pos + size).cmpgt(self.dim).any() {
            return None;
        }

        let new_lhs = pos;
        let new_rhs = pos + size;

        let mut distance = self.dim - pos - size;
        let mut to_clear = Vec::new();

        // check for intersections with live items
        for current_item in self.live_items.values() {
            let cur_lhs = current_item.position;
            let cur_rhs = current_item.position + current_item.size;

            let intersects = new_lhs.cmplt(cur_rhs) & new_rhs.cmpgt(cur_lhs);

            if intersects.all() {
                return None;
            }

            if intersects.y && intersects.z && cur_lhs.x > new_rhs.x {
                let distance_x = cur_lhs.x - new_rhs.x;
                if distance_x < distance.x {
                    distance.x = distance_x;
                }
            }

            if intersects.x && intersects.z && cur_lhs.y > new_rhs.y {
                let distance_y = cur_lhs.y - new_rhs.y;
                if distance_y < distance.y {
                    distance.y = distance_y;
                }
            }

            if intersects.x && intersects.y && cur_lhs.z > new_rhs.z {
                let distance_z = cur_lhs.z - new_rhs.z;
                if distance_z < distance.z {
                    distance.z = distance_z;
                }
            }
        }

        // check for intersections with dead items
        for (dead_handle, dead_item) in self.dead_items.iter() {
            let cur_lhs = dead_item.position;
            let cur_rhs = dead_item.position + dead_item.size;

            let intersects = new_lhs.cmplt(cur_rhs) & new_rhs.cmpgt(cur_lhs);

            if intersects.all() {
                to_clear.push(dead_handle.clone());
            }
        }

        Some((distance.x + distance.y + distance.z, to_clear))
    }

    pub fn insert(&mut self, handle: H, size: UVec3) -> Slot {
        if let Some(info) = self.live_items.get(&handle) {
            assert_eq!(size, info.size);
            return Slot::Existing(info.position);
        }

        if let Some(info) = self.dead_items.remove(&handle) {
            if size == info.size {
                // back from the dead
                self.live_items.insert(handle, info);
                return Slot::Existing(info.position);
            }

            // otherwise remove from dead and carry on
        }

        let (mut best_point, mut best_distance, mut best_evict_count, mut evictions) =
            (None, u32::MAX, usize::MAX, Vec::new());

        let mut insert_points = vec![UVec3::ZERO];
        for item in self.live_items.values() {
            insert_points.extend([
                item.position + item.size * UVec3::X,
                item.position + item.size * UVec3::Y,
                item.position + item.size * UVec3::Z,
            ]);
        }
        for item in self.dead_items.values() {
            insert_points.extend([
                item.position + item.size * UVec3::X,
                item.position + item.size * UVec3::Y,
                item.position + item.size * UVec3::Z,
            ]);
        }

        for insert_point in insert_points {
            if let Some((insert_distance, insert_evictions)) = self.measure(insert_point, size) {
                if insert_evictions.len() < best_evict_count
                    || insert_evictions.len() == best_evict_count && insert_distance < best_distance
                {
                    best_point = Some(insert_point);
                    best_distance = insert_distance;
                    best_evict_count = insert_evictions.len();
                    evictions = insert_evictions;
                }
            }
        }

        match best_point {
            Some(position) => {
                self.live_items.insert(handle, AtlasInfo { size, position });
                for item in evictions {
                    self.dead_items.remove(&item);
                }

                Slot::New(position)
            }
            None => Slot::NoFit,
        }
    }

    pub fn get(&self, handle: &H) -> Option<AtlasInfo> {
        self.live_items.get(handle).copied()
    }

    // mark as dead, keep around in case it gets added back
    pub fn remove(&mut self, handle: &H) {
        if let Some((key, info)) = self.live_items.remove_entry(handle) {
            self.dead_items.insert(key, info);
        }
    }

    // remove without keeping in reserve
    pub fn purge(&mut self, handle: &H) {
        self.live_items.remove(handle);
        self.dead_items.remove(handle);
    }

    // mark all handles as dead
    pub fn remove_all(&mut self) {
        self.dead_items.extend(self.live_items.drain())
    }

    // purge all
    pub fn purge_all(&mut self) {
        self.live_items.clear();
        self.dead_items.clear();
    }
}
