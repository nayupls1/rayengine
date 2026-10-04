use std::collections::{BTreeMap, BTreeSet};

/// Internal dimension-independent grid. Oversized entries/queries fall back to
/// bounds scanning, avoiding unbounded cell allocation for floors or fast sweeps.
#[derive(Debug)]
pub(super) struct Grid<const N: usize> {
    size: f32,
    bounds: BTreeMap<u64, ([f32; N], [f32; N])>,
    cells: BTreeMap<[i32; N], BTreeSet<u64>>,
    large: BTreeSet<u64>,
}
impl<const N: usize> Grid<N> {
    pub(super) fn new(size: f32) -> Self {
        assert!(
            size.is_finite() && size > 0.0,
            "cell size must be finite and positive"
        );
        Self {
            size,
            bounds: BTreeMap::new(),
            cells: BTreeMap::new(),
            large: BTreeSet::new(),
        }
    }
    fn cells_for(&self, min: [f32; N], max: [f32; N]) -> Option<Vec<[i32; N]>> {
        let low = min.map(|x| (x / self.size).floor() as i32);
        let high = max.map(|x| (x / self.size).floor() as i32);
        let count = (0..N).try_fold(1_u64, |count, i| {
            count.checked_mul((i64::from(high[i]) - i64::from(low[i]) + 1) as u64)
        })?;
        if count > 4096 {
            return None;
        }
        let mut cells = vec![low];
        for axis in 0..N {
            let mut next = Vec::new();
            for cell in cells {
                for coordinate in low[axis]..=high[axis] {
                    let mut cell = cell;
                    cell[axis] = coordinate;
                    next.push(cell);
                }
            }
            cells = next;
        }
        Some(cells)
    }
    pub(super) fn clear(&mut self) {
        self.cells.clear();
        self.bounds.clear();
        self.large.clear();
    }
    pub(super) fn insert(&mut self, id: u64, min: [f32; N], max: [f32; N]) {
        assert!(
            (0..N).all(|i| min[i].is_finite() && max[i].is_finite() && min[i] <= max[i]),
            "invalid grid bounds"
        );
        self.remove(id);
        if let Some(cells) = self.cells_for(min, max) {
            for cell in cells {
                self.cells.entry(cell).or_default().insert(id);
            }
        } else {
            self.large.insert(id);
        }
        self.bounds.insert(id, (min, max));
    }
    pub(super) fn remove(&mut self, id: u64) {
        if let Some((min, max)) = self.bounds.remove(&id) {
            if let Some(cells) = self.cells_for(min, max) {
                for cell in cells {
                    if let Some(entries) = self.cells.get_mut(&cell) {
                        entries.remove(&id);
                        if entries.is_empty() {
                            self.cells.remove(&cell);
                        }
                    }
                }
            }
            self.large.remove(&id);
        }
    }
    pub(super) fn query(&self, min: [f32; N], max: [f32; N]) -> Vec<u64> {
        assert!(
            (0..N).all(|i| min[i].is_finite() && max[i].is_finite() && min[i] <= max[i]),
            "invalid grid query"
        );
        let mut ids = self.large.clone();
        if let Some(cells) = self.cells_for(min, max) {
            for cell in cells {
                if let Some(entries) = self.cells.get(&cell) {
                    ids.extend(entries);
                }
            }
        } else {
            ids.extend(self.bounds.keys());
        }
        ids.into_iter()
            .filter(|id| {
                let (lo, hi) = self.bounds[id];
                (0..N).all(|i| min[i] <= hi[i] && max[i] >= lo[i])
            })
            .collect()
    }
}
