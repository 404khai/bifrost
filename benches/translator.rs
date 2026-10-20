mod support;

use std::hint::black_box;
use std::time::{Duration, Instant};

use bifrost::Revision;
use serde_json::{Value, json};
use support::{Case, registry, specs, target};

fn summarize(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    json!({"median_ns": at(0.5), "p05_ns": at(0.05), "p95_ns": at(0.95),
           "min_ns": sorted[0], "max_ns": sorted[sorted.len()-1], "samples_ns": samples})
}

fn measure(mut operation: impl FnMut(), samples: usize, operations: usize) -> (Value, usize) {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_millis(5) {
        operation();
    }
    let start = Instant::now();
    operation();
    let elapsed = start.elapsed().as_nanos().max(1);
    let batch = (2_000_000u128 / elapsed).clamp(1, 1024) as usize;
    let mut raw = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..batch {
            operation();
        }
        raw.push(start.elapsed().as_nanos() as f64 / (batch * operations) as f64);
    }
    (summarize(&raw), batch)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |key: &str| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
    };
    let samples: usize = get("--samples").map(|s| s.parse().unwrap()).unwrap_or(31);
    assert!(samples >= 5);
    let filter = get("--filter");
    if let Some(name) = get("--profile") {
        let spec = specs().into_iter().find(|s| s.name == name).unwrap();
        let case = Case::new(spec);
        case.verify();
        eprintln!("profile pid={}", std::process::id());
        let duration: u64 = get("--seconds").map(|s| s.parse().unwrap()).unwrap_or(10);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(duration) {
            case.operation("stream_e2e");
        }
        return;
    }
    let mut rows = Vec::new();
    for spec in specs()
        .into_iter()
        .filter(|s| filter.is_none_or(|f| s.name.contains(f)))
    {
        let case = Case::new(spec);
        case.verify();
        for method in ["value_e2e", "stream_e2e"] {
            let (mut row, batch) = measure(|| case.operation(method), samples, spec.clients);
            row["case"] = json!(spec.name);
            row["method"] = json!(method);
            row["bytes"] = json!(case.input.len());
            row["depth"] = json!(spec.depth);
            row["rules"] = json!(spec.rules);
            row["hops"] = json!(spec.hops);
            row["clients"] = json!(spec.clients);
            row["batch"] = json!(batch);
            row["mib_per_s"] = json!(
                case.input.len() as f64 * 1e9 / row["median_ns"].as_f64().unwrap() / 1048576.0
            );
            rows.push(row);
        }
    }
    let base = specs()[1];
    for hops in [0, 1, 3] {
        let mut registry = registry(base);
        registry.compile(Revision::V1, target(hops)).unwrap();
        let (mut row, batch) = measure(
            || {
                black_box(registry.compile(Revision::V1, target(hops)).unwrap());
            },
            samples,
            1,
        );
        row["case"] = json!(format!("hops_{hops}"));
        row["method"] = json!("cache_hit");
        row["batch"] = json!(batch);
        rows.push(row);
        let mut raw = Vec::new();
        const COLD_BATCH: usize = 32;
        for _ in 0..samples {
            // Prepare independent cold registries and result capacity before timing.
            // Batching avoids measuring a single operation at timer-quantum scale.
            let mut registries: Vec<_> = (0..COLD_BATCH).map(|_| support::registry(base)).collect();
            let mut plans = Vec::with_capacity(COLD_BATCH);
            let start = Instant::now();
            for registry in &mut registries {
                plans.push(registry.compile(Revision::V1, target(hops)).unwrap());
            }
            raw.push(start.elapsed().as_nanos() as f64 / COLD_BATCH as f64);
            black_box(plans);
        }
        let mut row = summarize(&raw);
        row["case"] = json!(format!("hops_{hops}"));
        row["method"] = json!("cold_compile");
        row["batch"] = json!(COLD_BATCH);
        rows.push(row);
    }
    println!(
        "{}",
        json!({"seed": support::SEED, "samples": samples, "rows": rows})
    );
}
