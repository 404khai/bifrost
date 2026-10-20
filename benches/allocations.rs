mod support;

use serde_json::json;
use support::{Case, specs};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |key: &str| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
    };
    let name = get("--case").expect("--case NAME required");
    let method = get("--method").expect("--method value_e2e|stream_e2e required");
    let spec = specs()
        .into_iter()
        .find(|s| s.name == name)
        .expect("unknown case");
    let case = Case::new(spec);
    case.verify();
    // Start after fixtures, plans, and shared client handles have been built.
    let profiler = if let Some(path) = get("--heap-profile") {
        dhat::Profiler::builder().file_name(path).build()
    } else {
        dhat::Profiler::builder().testing().build()
    };
    case.operation(method);
    let stats = dhat::HeapStats::get();
    drop(profiler);
    println!(
        "{}",
        json!({"case":name, "method":method, "bytes":case.input.len(), "clients":spec.clients,
        "total_blocks":stats.total_blocks, "total_bytes":stats.total_bytes,
        "peak_live_bytes":stats.max_bytes, "peak_live_blocks":stats.max_blocks,
        "end_live_bytes":stats.curr_bytes})
    );
}
