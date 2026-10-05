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
    let p: [f64; N] = std::array::from_fn(|i| f64::from(pa[i]) - f64::from(pb[i]));
    match (a, b) {
        (Shape::Box(ha), Shape::Box(hb)) => {
            let depths: [f64; N] =
                std::array::from_fn(|i| f64::from(ha[i]) + f64::from(hb[i]) - p[i].abs());
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
                depth: depths[axis] as f32,
            })
        }
        (Shape::Round(ra), Shape::Round(rb)) => {
            let distance = p.iter().map(|v| v * v).sum::<f64>().sqrt();
            let depth = f64::from(ra) + f64::from(rb) - distance;
            if depth <= 0.0 {
                return None;
            }
            let normal = if distance > 0.0 {
                p.map(|v| (v / distance) as f32)
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
        (Shape::Round(r), Shape::Box(h)) => round_box(p, f64::from(r), h.map(f64::from)),
        (Shape::Box(h), Shape::Round(r)) => {
            round_box(p.map(|v| -v), f64::from(r), h.map(f64::from)).map(|c| Contact {
                normal: scale(c.normal, -1.0),
                depth: c.depth,
            })
        }
    }
}
fn round_box<const N: usize>(p: [f64; N], r: f64, h: [f64; N]) -> Option<Contact<N>> {
    let delta: [f64; N] = std::array::from_fn(|i| p[i] - p[i].clamp(-h[i], h[i]));
    let distance = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    if distance > 0.0 {
        let depth = r - distance;
        return (depth > 0.0).then(|| Contact {
            normal: delta.map(|v| (v / distance) as f32),
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
        depth: (r + h[axis] - p[axis].abs()) as f32,
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
    sweep_precise(a, pa, b, pb, delta.map(f64::from)).map(|(time, normal)| (time as f32, normal))
}

// Keep relative subtraction, extents and travel fractions wide until the
// caller has selected the first collider and constructed its contact position.
pub(super) fn sweep_precise<const N: usize>(
    a: Shape<N>,
    pa: [f32; N],
    b: Shape<N>,
    pb: [f32; N],
    delta: [f64; N],
) -> Option<(f64, [f32; N])> {
    let p: [f64; N] = std::array::from_fn(|i| f64::from(pa[i]) - f64::from(pb[i]));
    match (a, b) {
        (Shape::Box(ha), Shape::Box(hb)) => {
            let h: [f64; N] = std::array::from_fn(|i| f64::from(ha[i]) + f64::from(hb[i]));
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
                    let t1 = (-h[i] - p[i]) / delta[i];
                    let t2 = (h[i] - p[i]) / delta[i];
                    let lo = t1.min(t2);
                    if lo >= enter {
                        enter = lo;
                        normal = [0.0; N];
                        normal[i] = -delta[i].signum() as f32;
                    }
                    exit = exit.min(t1.max(t2));
                }
            }
            (enter < exit
                && enter <= 1.0
                && (0..N).map(|i| f64::from(normal[i]) * delta[i]).sum::<f64>() < 0.0)
                .then_some((enter, normal))
        }
        (Shape::Round(ra), Shape::Round(rb)) => {
            rounded_sweep(p, delta, [0.0; N], f64::from(ra) + f64::from(rb))
        }
        (Shape::Round(r), Shape::Box(h)) => rounded_sweep(p, delta, h.map(f64::from), f64::from(r)),
        (Shape::Box(h), Shape::Round(r)) => rounded_sweep(
            p.map(|v| -v),
            delta.map(|v| -v),
            h.map(f64::from),
            f64::from(r),
        )
        .map(|(t, n)| (t, scale(n, -1.0))),
    }
}

// Squared distance to an AABB is quadratic between crossings of its faces.
// Solving those intervals gives exact face, edge and corner CCD for rounds.
fn rounded_sweep<const N: usize>(
    p: [f64; N],
    d: [f64; N],
    h: [f64; N],
    r: f64,
) -> Option<(f64, [f32; N])> {
    let surface_offset = |time: f64| -> [f64; N] {
        std::array::from_fn(|i| {
            let at = p[i] + d[i] * time;
            at - at.clamp(-h[i], h[i])
        })
    };
    let initial = surface_offset(0.0);
    if initial.iter().map(|v| v * v).sum::<f64>() == r * r
        && (0..N).map(|i| initial[i] * d[i]).sum::<f64>() < 0.0
    {
        return Some((0.0, initial.map(|v| (v / r) as f32)));
    }
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
        // Gram's identity expresses the discriminant without subtracting
        // large position squares or evaluating a rounded vertex distance:
        // r²*|d|² - sum((offset_i*d_j - offset_j*d_i)²).
        // It retains small radii on distant rays and exact oblique tangencies.
        let mut perpendicular = 0.0;
        for i in 0..N {
            for j in i + 1..N {
                if active[i] && active[j] {
                    let cross = offset[i] * d[j] - offset[j] * d[i];
                    perpendicular += cross * cross;
                }
            }
        }
        let discriminant = r * r * qa - perpendicular;
        if discriminant <= 0.0 {
            continue; // Tangency has no inward velocity.
        }
        let span = discriminant.sqrt() / qa;
        let mut t = vertex - span;
        if t > 1.0 {
            // Snap a rounded endpoint root only if the actual endpoint reaches
            // the rounded geometry; a near miss beyond travel still misses.
            let end = surface_offset(1.0);
            if end.iter().map(|v| v * v).sum::<f64>() <= r * r {
                t = 1.0;
            }
        }
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        // Never extend a face equation into a different surface region. Only
        // snap a root a few f64 ULPs outside its interval when the actual
        // boundary reaches the rounded geometry. A fixed fraction tolerance
        // spans entire unrelated faces on long casts.
        let boundary = t.clamp(interval[0], interval[1]);
        let snapped = boundary != t;
        if snapped {
            let tolerance = 4.0 * f64::EPSILON * t.abs().max(boundary.abs());
            let offset = surface_offset(boundary);
            if (t - boundary).abs() > tolerance || offset.iter().map(|v| v * v).sum::<f64>() > r * r
            {
                continue;
            }
            t = boundary;
        }
        // Recover the normal near the vertex too, avoiding p + d*t cancellation.
        let mut normal64: [f64; N] = std::array::from_fn(|i| {
            if active[i] {
                at_vertex[i] - d[i] * span
            } else {
                0.0
            }
        });
        if snapped {
            normal64 = surface_offset(t);
        }
        let length = normal64.iter().map(|x| x * x).sum::<f64>().sqrt();
        if length == 0.0 {
            continue;
        }
        let normal = normal64.map(|x| (x / length) as f32);
        if (0..N).map(|i| f64::from(normal[i]) * d[i]).sum::<f64>() < 0.0 {
            return Some((t, normal));
        }
    }
    None
}
