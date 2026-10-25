//! Computed maps, kept in memory and (optionally) on disk.
//!
//! A map does not change within a day for a given station and model, so a
//! fine grid computed once is worth keeping: on disk each is 69 KB
//! (reliability and take-off angle quantised to a byte each), against the
//! seconds of battery a phone spends recomputing it.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::coverage::Grid;

/// A small least-recently-used map.
pub struct Lru<V> {
    capacity: usize,
    order: VecDeque<String>,
    items: HashMap<String, Arc<V>>,
}

impl<V> Lru<V> {
    pub fn new(capacity: usize) -> Lru<V> {
        Lru {
            capacity,
            order: VecDeque::new(),
            items: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<V>> {
        let v = self.items.get(key)?.clone();
        self.order.retain(|k| k != key);
        self.order.push_back(key.to_string());
        Some(v)
    }

    pub fn put(&mut self, key: String, value: Arc<V>) {
        self.order.retain(|k| *k != key);
        self.order.push_back(key.clone());
        self.items.insert(key, value);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.items.remove(&old);
            }
        }
    }

    pub fn clear(&mut self) {
        self.order.clear();
        self.items.clear();
    }
}

/// FNV-1a: a file name that is the same on every build and platform (std's
/// hasher makes no such promise).
pub fn fnv64(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

const MAGIC: &[u8; 4] = b"HFG1";

/// Grids on disk, up to a size budget; the least recently used go first.
pub struct DiskCache {
    dir: PathBuf,
    budget_bytes: u64,
}

impl DiskCache {
    pub fn new(dir: PathBuf, budget_bytes: u64) -> DiskCache {
        DiskCache { dir, budget_bytes }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.grid", fnv64(key)))
    }

    pub fn get(&self, key: &str) -> Option<Grid> {
        let path = self.path(key);
        let bytes = fs::read(&path).ok()?;
        let grid = decode(&bytes, key)?;
        // Touch it, so pruning keeps what is in use.
        let _ = fs::File::options()
            .append(true)
            .open(&path)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()));
        Some(grid)
    }

    pub fn put(&self, key: &str, grid: &Grid) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let path = self.path(key);
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, encode(grid, key)).is_ok() && fs::rename(&tmp, &path).is_ok() {
            self.prune();
        }
    }

    /// Bytes held now.
    pub fn size(&self) -> u64 {
        self.entries().iter().map(|e| e.1).sum()
    }

    pub fn clear(&self) {
        for (path, _, _) in self.entries() {
            let _ = fs::remove_file(path);
        }
    }

    fn entries(&self) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
        let Ok(dir) = fs::read_dir(&self.dir) else {
            return vec![];
        };
        dir.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "grid"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((e.path(), m.len(), m.modified().ok()?))
            })
            .collect()
    }

    fn prune(&self) {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        if total <= self.budget_bytes {
            return;
        }
        entries.sort_by_key(|e| e.2);
        for (path, len, _) in entries {
            if total <= self.budget_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

/// Header, the key (so a hash collision reads as a miss), then one byte of
/// reliability and one of angle per cell.
fn encode(g: &Grid, key: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + key.len() + g.reliability.len() * 2);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    for v in [g.lat_min, g.lon_min, g.lat_step, g.lon_step] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(g.nx as u32).to_le_bytes());
    out.extend_from_slice(&(g.ny as u32).to_le_bytes());
    out.extend(
        g.reliability
            .iter()
            .map(|r| (r.clamp(0.0, 1.0) * 250.0).round() as u8),
    );
    out.extend(g.takeoff_deg.iter().map(|a| {
        if a.is_nan() {
            255
        } else {
            (a.clamp(0.0, 90.0) * 2.0).round() as u8
        }
    }));
    out
}

fn decode(bytes: &[u8], key: &str) -> Option<Grid> {
    let mut at = 0;
    let mut take = |n: usize| {
        let s = bytes.get(at..at + n)?;
        at += n;
        Some(s)
    };
    if take(4)? != MAGIC {
        return None;
    }
    let klen = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    if take(klen)? != key.as_bytes() {
        return None;
    }
    let mut f = || Some(f64::from_le_bytes(take(8)?.try_into().ok()?));
    let (lat_min, lon_min, lat_step, lon_step) = (f()?, f()?, f()?, f()?);
    let nx = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    let ny = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    let n = nx.checked_mul(ny)?;
    let reliability = take(n)?.iter().map(|&b| f32::from(b) / 250.0).collect();
    let takeoff_deg = take(n)?
        .iter()
        .map(|&b| {
            if b == 255 {
                f32::NAN
            } else {
                f32::from(b) / 2.0
            }
        })
        .collect();
    Some(Grid {
        lat_min,
        lon_min,
        lat_step,
        lon_step,
        nx,
        ny,
        reliability,
        takeoff_deg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_round_trip_and_prune() {
        let dir = std::env::temp_dir().join("hfp-cache-test");
        let _ = fs::remove_dir_all(&dir);
        let grid = Grid {
            lat_min: -89.375,
            lon_min: -179.25,
            lat_step: 1.25,
            lon_step: 1.5,
            nx: 3,
            ny: 2,
            reliability: vec![0.0, 0.5, 1.0, 0.72, 0.4, 0.15],
            takeoff_deg: vec![f32::NAN, 3.5, 88.0, 60.0, 0.0, 12.5],
        };
        let cache = DiskCache::new(dir.clone(), 1_000_000);
        cache.put("a", &grid);
        let back = cache.get("a").unwrap();
        assert_eq!(back.nx, 3);
        assert_eq!(back.reliability[3], 0.72);
        assert!(back.takeoff_deg[0].is_nan() && back.takeoff_deg[5] == 12.5);
        assert!(cache.get("b").is_none());
        // A tiny budget keeps only the newest.
        let small = DiskCache::new(dir.clone(), 100);
        small.put("b", &grid);
        assert!(small.get("a").is_none() && small.get("b").is_some());
        small.clear();
        assert_eq!(small.size(), 0);
    }

    #[test]
    fn lru_evicts_oldest() {
        let mut lru = Lru::new(2);
        lru.put("a".into(), Arc::new(1));
        lru.put("b".into(), Arc::new(2));
        lru.get("a");
        lru.put("c".into(), Arc::new(3));
        assert!(lru.get("b").is_none() && lru.get("a").is_some() && lru.get("c").is_some());
    }
}
