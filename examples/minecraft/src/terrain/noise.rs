//! Version-one integer value noise. Every shift/division/constant is recipe ABI.
use rayengine_voxel::BlockPos;
const ONE: i64 = 65536;
pub(super) fn hash(seed: u64, salt: u64, x: i64, y: i64, z: i64) -> u64 {
    let mut h = seed ^ salt;
    h ^= (x as u64).wrapping_mul(0x9e3779b97f4a7c15);
    h ^= (y as u64).wrapping_mul(0xd1b54a32d192ed03);
    h ^= (z as u64).wrapping_mul(0x94d049bb133111eb);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h ^ (h >> 31)
}
fn value(seed: u64, salt: u64, p: [i64; 3]) -> i32 {
    (hash(seed, salt, p[0], p[1], p[2]) >> 48) as i32 - 32768
}
fn blend(a: i32, b: i32, t: i64) -> i32 {
    (i64::from(a) + (((i64::from(b) - i64::from(a)) * t) >> 16)) as i32
}
fn fraction(p: i64, scale: i64) -> i64 {
    let t = p.rem_euclid(scale) * ONE / scale;
    let square = (t * t) >> 16;
    (square * (3 * ONE - 2 * t)) >> 16
}
pub(super) fn noise2(seed: u64, salt: u64, x: i64, z: i64, scale: i64) -> i32 {
    let [gx, gz] = [x.div_euclid(scale), z.div_euclid(scale)];
    let [tx, tz] = [fraction(x, scale), fraction(z, scale)];
    let a = blend(
        value(seed, salt, [gx, 0, gz]),
        value(seed, salt, [gx + 1, 0, gz]),
        tx,
    );
    let b = blend(
        value(seed, salt, [gx, 0, gz + 1]),
        value(seed, salt, [gx + 1, 0, gz + 1]),
        tx,
    );
    blend(a, b, tz)
}
pub(super) fn noise3(seed: u64, salt: u64, p: [i64; 3], scale: [i64; 3]) -> i32 {
    let grid = std::array::from_fn::<_, 3, _>(|i| p[i].div_euclid(scale[i]));
    interpolate(
        std::array::from_fn(|i| fraction(p[i], scale[i])),
        |dx, dy, dz| value(seed, salt, [grid[0] + dx, grid[1] + dy, grid[2] + dz]),
    )
}
fn interpolate(t: [i64; 3], mut at: impl FnMut(i64, i64, i64) -> i32) -> i32 {
    let low = blend(
        blend(at(0, 0, 0), at(1, 0, 0), t[0]),
        blend(at(0, 1, 0), at(1, 1, 0), t[0]),
        t[1],
    );
    let high = blend(
        blend(at(0, 0, 1), at(1, 0, 1), t[0]),
        blend(at(0, 1, 1), at(1, 1, 1), t[0]),
        t[1],
    );
    blend(low, high, t[2])
}
/// At most 6³ cached lattice nodes for one 16³ chunk and scales >= 5.
pub(super) struct NoiseGrid {
    values: [i32; 216],
    origin: [i64; 3],
    dims: [usize; 3],
    scale: [i64; 3],
}
impl NoiseGrid {
    pub(super) fn new(seed: u64, salt: u64, p: BlockPos, scale: [i64; 3]) -> Self {
        let p = [i64::from(p.x), i64::from(p.y), i64::from(p.z)];
        let origin = std::array::from_fn(|i| p[i].div_euclid(scale[i]));
        let dims =
            std::array::from_fn(|i| ((p[i] + 15).div_euclid(scale[i]) - origin[i] + 2) as usize);
        assert!(scale.iter().all(|s| *s >= 5) && dims.iter().all(|d| *d <= 6));
        let mut result = Self {
            values: [0; 216],
            origin,
            dims,
            scale,
        };
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let i = result.index(x, y, z);
                    result.values[i] = value(
                        seed,
                        salt,
                        [
                            origin[0] + x as i64,
                            origin[1] + y as i64,
                            origin[2] + z as i64,
                        ],
                    );
                }
            }
        }
        result
    }
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + self.dims[0] * (y + self.dims[1] * z)
    }
    pub(super) fn sample(&self, p: BlockPos) -> i32 {
        let p = [i64::from(p.x), i64::from(p.y), i64::from(p.z)];
        let grid = std::array::from_fn::<_, 3, _>(|i| {
            (p[i].div_euclid(self.scale[i]) - self.origin[i]) as usize
        });
        interpolate(
            std::array::from_fn(|i| fraction(p[i], self.scale[i])),
            |dx, dy, dz| {
                self.values[self.index(
                    grid[0] + dx as usize,
                    grid[1] + dy as usize,
                    grid[2] + dz as usize,
                )]
            },
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_noise_matches_direct_noise_at_negative_and_extreme_coordinates() {
        for origin in [
            BlockPos::new(-32, -16, -48),
            BlockPos::default(),
            BlockPos::new(i32::MAX - 15, i32::MIN, i32::MAX - 15),
        ] {
            for scale in [[24, 16, 24], [11, 9, 11], [6; 3], [7; 3]] {
                let grid = NoiseGrid::new(u64::MAX, 17, origin, scale);
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let p = BlockPos::new(origin.x + x, origin.y + y, origin.z + z);
                            assert_eq!(
                                grid.sample(p),
                                noise3(u64::MAX, 17, [p.x.into(), p.y.into(), p.z.into()], scale)
                            );
                        }
                    }
                }
            }
        }
    }
}
