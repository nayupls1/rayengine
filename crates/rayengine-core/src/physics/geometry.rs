use super::Contact;

#[derive(Clone, Copy, Debug)]
pub(super) enum Shape<const N: usize> {
    Box([f32; N]),
    Round(f32),
}

pub(super) fn add<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(super) fn sub<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn scale<const N: usize>(a: [f32; N], s: f32) -> [f32; N] {
    a.map(|v| v * s)
}
pub(super) fn dot<const N: usize>(a: [f32; N], b: [f32; N]) -> f32 {
    (0..N).map(|i| a[i] * b[i]).sum()
}
// Outward correction must advance even when a contact depth/skin is smaller
// than one f32 ULP at the current coordinate. Static/zero-weight corrections
// pass zero distance and remain unchanged.
pub(super) fn outward<const N: usize>(
    position: [f32; N],
    normal: [f32; N],
    distance: f32,
) -> [f32; N] {
    let delta = scale(normal, distance);
    let mut result = add(position, delta);
    for i in 0..N {
        if delta[i] > 0.0 && result[i] <= position[i] {
            result[i] = position[i].next_up();
        } else if delta[i] < 0.0 && result[i] >= position[i] {
            result[i] = position[i].next_down();
        }
    }
    result
}
impl<const N: usize> Shape<N> {
    pub(super) fn half(self) -> [f32; N] {
        match self {
            Self::Box(h) => h,
            Self::Round(r) => [r; N],
        }
    }
    pub(super) fn valid(self) -> bool {
        self.half().iter().all(|v| v.is_finite() && *v > 0.0)
    }
    pub(super) fn bounds(self, p: [f32; N]) -> ([f32; N], [f32; N]) {
        (sub(p, self.half()), add(p, self.half()))
    }
}

// Normal always points from B toward A: moving A along it separates the pair.
pub(super) fn overlap<const N: usize>(
    a: Shape<N>,
    pa: [f32; N],
    b: Shape<N>,
    pb: [f32; N],
) -> Option<Contact<N>> {
    let p = sub(pa, pb);
    match (a, b) {
        (Shape::Box(ha), Shape::Box(hb)) => {
            let extent = add(ha, hb);
            let depths = std::array::from_fn::<_, N, _>(|i| extent[i] - p[i].abs());
            if depths.iter().any(|d| *d <= 0.0) {
                return None;
            }
            let axis = (0..N)
                .min_by(|&i, &j| depths[i].total_cmp(&depths[j]))
                .unwrap();
            let mut normal = [0.0; N];
            normal[axis] = if p[axis] < 0.0 { -1.0 } else { 1.0 };
            Some(Contact {
                normal,
                depth: depths[axis],
            })
        }
        (Shape::Round(ra), Shape::Round(rb)) => {
            let distance = p.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
            let depth = f64::from(ra) + f64::from(rb) - distance;
            if depth <= 0.0 {
                return None;
            }
            let normal = if distance > 0.0 {
                p.map(|v| (f64::from(v) / distance) as f32)
            } else {
                let mut n = [0.0; N];
                n[0] = 1.0;
                n
            };
            Some(Contact {
                normal,
                depth: depth as f32,
            })
        }
        (Shape::Round(r), Shape::Box(h)) => round_box(p, r, h),
        (Shape::Box(h), Shape::Round(r)) => round_box(scale(p, -1.0), r, h).map(|c| Contact {
            normal: scale(c.normal, -1.0),
            depth: c.depth,
        }),
    }
}
fn round_box<const N: usize>(p: [f32; N], r: f32, h: [f32; N]) -> Option<Contact<N>> {
    let closest = std::array::from_fn(|i| p[i].clamp(-h[i], h[i]));
    let delta = sub(p, closest);
    let distance = delta
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    if distance > 0.0 {
        let depth = f64::from(r) - distance;
        return (depth > 0.0).then(|| Contact {
            normal: delta.map(|v| (f64::from(v) / distance) as f32),
            depth: depth as f32,
        });
    }
    let axis = (0..N)
        .min_by(|&i, &j| (h[i] - p[i].abs()).total_cmp(&(h[j] - p[j].abs())))
        .unwrap();
    let mut normal = [0.0; N];
    normal[axis] = if p[axis] < 0.0 { -1.0 } else { 1.0 };
    Some(Contact {
        normal,
        depth: r + h[axis] - p[axis].abs(),
    })
}

/// Earliest entering contact during relative linear motion. Uses f64 roots so
/// a fast projectile does not lose thin geometry through quadratic cancellation.
pub(super) fn sweep<const N: usize>(
    a: Shape<N>,
    pa: [f32; N],
    b: Shape<N>,
    pb: [f32; N],
    delta: [f32; N],
) -> Option<(f32, [f32; N])> {
    let p = sub(pa, pb);
    match (a, b) {
        (Shape::Box(ha), Shape::Box(hb)) => {
            let h = add(ha, hb);
            let mut enter = 0.0_f64;
            let mut exit = f64::INFINITY;
            let mut normal = [0.0; N];
            for i in 0..N {
                if delta[i] == 0.0 {
                    // A parallel grazing path is not an entering collision.
                    if p[i].abs() >= h[i] {
                        return None;
                    }
                } else {
                    let t1 = (-f64::from(h[i]) - f64::from(p[i])) / f64::from(delta[i]);
                    let t2 = (f64::from(h[i]) - f64::from(p[i])) / f64::from(delta[i]);
                    let lo = t1.min(t2);
                    if lo >= enter {
                        enter = lo;
                        normal = [0.0; N];
                        normal[i] = -delta[i].signum();
                    }
                    exit = exit.min(t1.max(t2));
                }
            }
            (enter < exit && enter <= 1.0 && dot(normal, delta) < 0.0)
                .then_some((enter as f32, normal))
        }
        (Shape::Round(ra), Shape::Round(rb)) => rounded_sweep(p, delta, [0.0; N], ra + rb),
        (Shape::Round(r), Shape::Box(h)) => rounded_sweep(p, delta, h, r),
        (Shape::Box(h), Shape::Round(r)) => rounded_sweep(scale(p, -1.0), scale(delta, -1.0), h, r)
            .map(|(t, n)| (t, scale(n, -1.0))),
    }
}

// Squared distance to an AABB is quadratic between crossings of its faces.
// Solving those intervals gives exact face, edge and corner CCD for rounds.
fn rounded_sweep<const N: usize>(
    p: [f32; N],
    delta: [f32; N],
    half: [f32; N],
    radius: f32,
) -> Option<(f32, [f32; N])> {
    let p = p.map(f64::from);
    let d = delta.map(f64::from);
    let h = half.map(f64::from);
    let r = f64::from(radius);
    let mut cuts = vec![0.0, 1.0];
    for i in 0..N {
        if d[i] != 0.0 {
            for face in [-h[i], h[i]] {
                let t = (face - p[i]) / d[i];
                if t > 0.0 && t < 1.0 {
                    cuts.push(t);
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    for interval in cuts.windows(2) {
        let mid = (interval[0] + interval[1]) * 0.5;
        let active: [bool; N] = std::array::from_fn(|i| (p[i] + d[i] * mid).abs() > h[i]);
        let offset: [f64; N] = std::array::from_fn(|i| {
            if active[i] {
                p[i] - h[i].copysign(p[i] + d[i] * mid)
            } else {
                0.0
            }
        });
        let qa: f64 = (0..N).filter(|&i| active[i]).map(|i| d[i] * d[i]).sum();
        if qa == 0.0 {
            continue;
        }
        let qb: f64 = (0..N)
            .filter(|&i| active[i])
            .map(|i| 2.0 * offset[i] * d[i])
            .sum();
        let vertex = -qb / (2.0 * qa);
        let at_vertex: [f64; N] = std::array::from_fn(|i| {
            if active[i] {
                offset[i] + d[i] * vertex
            } else {
                0.0
            }
        });
        // Evaluate distance at the quadratic's vertex instead of subtracting
        // qb² - 4*qa*qc. That subtraction loses small radii on distant casts.
        let gap = r * r - at_vertex.iter().map(|v| v * v).sum::<f64>();
        if gap <= 0.0 {
            continue; // Tangency has no inward velocity.
        }
        let span = (gap / qa).sqrt();
        let t = vertex - span;
        if !(0.0..=1.0).contains(&t) || t < interval[0] - 1e-10 || t > interval[1] + 1e-10 {
            continue;
        }
        // Recover the normal near the vertex too, avoiding p + d*t cancellation.
        let normal64: [f64; N] = std::array::from_fn(|i| {
            if active[i] {
                at_vertex[i] - d[i] * span
            } else {
                0.0
            }
        });
        let length = normal64.iter().map(|x| x * x).sum::<f64>().sqrt();
        if length == 0.0 {
            continue;
        }
        let normal = normal64.map(|x| (x / length) as f32);
        if (0..N)
            .map(|i| f64::from(normal[i]) * f64::from(delta[i]))
            .sum::<f64>()
            < 0.0
        {
            return Some((t as f32, normal));
        }
    }
    None
}
