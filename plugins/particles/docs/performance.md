# Bounded-load performance

The stable `particles_cpu_v1` workloads compare idle steps, full-capacity steps,
240 continuous saturated ticks, rejected oversized bursts and admitted oversized
bursts at capacity 128, 4,096 and 32,768. Seeded fixture creation, vector allocation
and destruction occur outside timing. Birth admission is capped at capacity/4
per call. Full fixtures are filled through four bounded bursts. Lifetime is ten
seconds; no particle expires during the two-second sustained workload.

Storage is one reserved vector of 56-byte particles; rendering adds one reserved
`usize` index per capacity slot. On 64-bit Linux that is 8 KiB, 256 KiB and 2 MiB
of payload at the three capacities, plus fixed structures and one GPU quad/material
per initialized effect. Storage does not grow after construction. CPU regression
tests check the vector pointer/capacity across 10,000 saturated steps. Rejected
births perform no RNG work. Simulation is O(live particles + admitted births),
3D ordering is O(n log n), and 2D submission is O(n).

Reproduce or compare using the repository provenance/export workflow:

```sh
scripts/benchmark.sh save particles-v1 particles_
scripts/benchmark.sh compare particles-v1 particles_
```

The measurements are CPU-only, with no window/GPU and no claim about game FPS.
Save and compare use the same code to establish repeat-run noise and compare
bounded workload sizes; this is a new plugin with no previous implementation.

## Local measurements

Measured with Rust 1.98.1 on Linux-7.2.5-3-omarchy-x86_64-with-glibc2.44.
12th Gen Intel(R) Core(TM) i7-12700K. Criterion: 10 samples, 100 ms warmup, 300 ms requested measurement.
The machine was shared with other development workloads; short runs show
substantial noise. The ranges below span the two run means, not confidence
intervals. No optimization or regression claim is inferred from the repeat run.

| Capacity | Idle | Full tick | Saturated 240 ticks | Full burst rejection | Admit capacity/4 |
| --- | --- | --- | --- | --- | --- |
| 128 | 8.045–10.081 ns | 0.401–0.516 µs | 0.097–0.100 ms | 3.184–3.785 ns | 0.243–0.251 µs |
| 4096 | 9.612–10.740 ns | 12.511–15.446 µs | 2.983–3.223 ms | 17.099–24.661 ns | 7.639–10.988 µs |
| 32768 | 10.197–26.265 ns | 101.861–306.454 µs | 24.396–52.880 ms | 58.023–133.074 ns | 72.785–88.794 µs |

The largest full emitter remained at 32,768 live particles for all 240 ticks.
Oversized full bursts reject immediately at every capacity. Admitted burst cost
tracks admitted births, while saturated tick cost tracks the bounded live count.
Both runs use f64 age accumulation and 56-byte particle storage.
Raw repeat means, confidence intervals and machine provenance are in
[measurements.json](crate::guides::measurements).
