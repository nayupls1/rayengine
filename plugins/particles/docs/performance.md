# Bounded-load performance

The stable `particles_cpu_v1` workloads compare idle steps, full-capacity steps,
240 continuous saturated ticks, rejected oversized bursts and admitted oversized
bursts at capacity 128, 4,096 and 32,768. Seeded fixture creation, vector allocation
and destruction occur outside timing. Birth admission is capped at capacity/4
per call. Full fixtures are filled through four bounded bursts. Lifetime is ten
seconds; no particle expires during the two-second sustained workload.

Storage is one reserved vector of 48-byte particles; rendering adds one reserved
`usize` index per capacity slot. On 64-bit Linux that is 7 KiB, 224 KiB and 1.75 MiB
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

Measured with Rust 1.98.1 on x86_64, Linux 7.2.5-3-omarchy.
12th Gen Intel(R) Core(TM) i7-12700K. Criterion: 10 samples, 100 ms warmup, 300 ms requested measurement.
The machine was shared with other development workloads; short runs show
substantial noise. The ranges below span the two run means, not confidence
intervals. No optimization or regression claim is inferred from the repeat run.

| Capacity | Idle | Full tick | Saturated 240 ticks | Full burst rejection | Admit capacity/4 |
| --- | --- | --- | --- | --- | --- |
| 128 | 7.13–7.77 ns | 0.45–0.47 µs | 0.11–0.11 ms | 3.05–3.17 ns | 0.22–0.33 µs |
| 4096 | 9.52–17.87 ns | 14.31–20.64 µs | 3.54–4.61 ms | 20.77–32.39 ns | 11.13–11.93 µs |
| 32768 | 18.42–21.91 ns | 116.97–117.15 µs | 27.62–29.01 ms | 41.79–45.27 ns | 56.87–57.79 µs |

The largest full emitter remained at 32,768 live particles for all 240 ticks.
Oversized full bursts reject immediately at every capacity. Admitted burst cost
tracks admitted births, while saturated tick cost tracks the bounded live count.
Raw repeat means and machine metadata are in [measurements.json](measurements.json).
