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
            let distance = dot(p, p).sqrt();
            let depth = ra + rb - distance;
            if depth <= 0.0 {
                return None;
            }
            let normal = if distance > 0.0 {
                scale(p, 1.0 / distance)
            } else {
                let mut n = [0.0; N];
                n[0] = 1.0;
                n
            };
            Some(Contact { normal, depth })
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
    let distance = dot(delta, delta).sqrt();
    if distance > 0.0 {
        let depth = r - distance;
        return (depth > 0.0).then(|| Contact {
            normal: scale(delta, 1.0 / distance),
            depth,
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
            let mut exit = 1.0_f64;
            let mut normal = [0.0; N];
            for i in 0..N {
                if delta[i] == 0.0 {
                    // A parallel grazing path is not an entering collision.
                    if p[i].abs() >= h[i] {
                        return None;
                    }
                } else {
                    let t1 = f64::from(-h[i] - p[i]) / f64::from(delta[i]);
                    let t2 = f64::from(h[i] - p[i]) / f64::from(delta[i]);
                    let lo = t1.min(t2);
                    if lo >= enter {
                        enter = lo;
                        normal = [0.0; N];
                        normal[i] = -delta[i].signum();
                    }
                    exit = exit.min(t1.max(t2));
                }
            }
            (enter <= exit && enter <= 1.0 && dot(normal, delta) < 0.0)
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
        let mut qa = 0.0;
        let mut qb = 0.0;
        let mut qc = -r * r;
        for i in 0..N {
            let at = p[i] + d[i] * mid;
            if at.abs() > h[i] {
                let offset = p[i] - h[i].copysign(at);
                qa += d[i] * d[i];
                qb += 2.0 * offset * d[i];
                qc += offset * offset;
            }
        }
        if qa == 0.0 {
            continue;
        }
        let disc = qb * qb - 4.0 * qa * qc;
        if disc <= 0.0 {
            continue;
        } // Tangency has no inward velocity.
        let root = disc.sqrt();
        let q = -0.5 * (qb + root.copysign(qb));
        let t = (q / qa).min(qc / q);
        if t < interval[0] - 1e-10 || t > interval[1] + 1e-10 {
            continue;
        }
        let t = t.clamp(0.0, 1.0);
        let normal64: [f64; N] = std::array::from_fn(|i| {
            let at = p[i] + d[i] * t;
            at - at.clamp(-h[i], h[i])
        });
        let length = normal64.iter().map(|x| x * x).sum::<f64>().sqrt();
        if length == 0.0 {
            continue;
        }
        let normal = normal64.map(|x| (x / length) as f32);
        if dot(normal, delta) < 0.0 {
            return Some((t as f32, normal));
        }
    }
    None
}
