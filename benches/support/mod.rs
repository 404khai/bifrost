use std::hint::black_box;
use std::sync::Arc;

use bifrost::{CompiledChain, Pointer, Registry, Rename, Revision, StreamingLimits, Trie};
use serde_json::{Value, json};

pub const SEED: u64 = 0xB1F2057;

#[derive(Clone, Copy)]
pub struct Spec {
    pub name: &'static str,
    pub size: usize,
    pub depth: usize,
    pub rules: usize,
    pub hops: usize,
    pub clients: usize,
    pub blob: bool,
    pub changed_blob: bool,
}

pub fn specs() -> Vec<Spec> {
    let base = Spec {
        name: "objects_20k",
        size: 20 * 1024,
        depth: 2,
        rules: 1,
        hops: 1,
        clients: 1,
        blob: false,
        changed_blob: false,
    };
    vec![
        Spec {
            name: "objects_1k",
            size: 1024,
            ..base
        },
        base,
        Spec {
            name: "objects_1m",
            size: 1024 * 1024,
            ..base
        },
        Spec {
            name: "depth_12",
            depth: 12,
            ..base
        },
        Spec {
            name: "depth_32",
            depth: 32,
            ..base
        },
        Spec {
            name: "rules_8",
            rules: 8,
            ..base
        },
        Spec {
            name: "rules_32",
            rules: 32,
            ..base
        },
        Spec {
            name: "identity",
            hops: 0,
            ..base
        },
        Spec {
            name: "hops_3",
            hops: 3,
            ..base
        },
        Spec {
            name: "clients_8",
            clients: 8,
            ..base
        },
        Spec {
            name: "clients_64",
            clients: 64,
            ..base
        },
        Spec {
            name: "blob_unchanged_20k",
            blob: true,
            ..base
        },
        Spec {
            name: "blob_changed_20k",
            blob: true,
            changed_blob: true,
            ..base
        },
        Spec {
            name: "blob_unchanged_1m",
            size: 1024 * 1024,
            blob: true,
            ..base
        },
    ]
}

pub struct Case {
    pub spec: Spec,
    pub input: Vec<u8>,
    pub plan: Arc<CompiledChain>,
    pub clients: Vec<Arc<CompiledChain>>,
}

fn field(index: usize) -> &'static str {
    ["name", "title", "label", "display_name"][index]
}

pub fn target(hops: usize) -> Revision {
    [Revision::V1, Revision::V2, Revision::V3, Revision::V4][hops]
}

pub fn registry(spec: Spec) -> Registry {
    let mut registry = Registry::new([Revision::V1, Revision::V2, Revision::V3, Revision::V4]);
    for hop in 0..3 {
        let revisions = [Revision::V1, Revision::V2, Revision::V3, Revision::V4];
        for (from, to, source, destination) in [
            (
                revisions[hop],
                revisions[hop + 1],
                field(hop),
                field(hop + 1),
            ),
            (
                revisions[hop + 1],
                revisions[hop],
                field(hop + 1),
                field(hop),
            ),
        ] {
            let mut rules = Trie::new();
            for index in 0..spec.rules {
                let path = format!("{}/r{index}", "/level".repeat(spec.depth));
                rules.insert(
                    &Pointer::parse(&path).unwrap(),
                    Rename::new(source, destination),
                );
            }
            registry.set_transition(from, to, vec![rules]).unwrap();
        }
    }
    registry
}

fn fixture(spec: Spec) -> Vec<u8> {
    let mut seed = SEED;
    let mut root = serde_json::Map::new();
    for index in 0..spec.rules {
        root.insert(
            format!("r{index}"),
            json!({"name": "astrid", "status": "running", "tags": [true, null]}),
        );
    }
    let mut tree = Value::Object(root);
    for _ in 0..spec.depth {
        tree = json!({"level": tree});
    }
    tree["metadata"] = json!({"trace_id": "bifrost-benchmark", "seed": SEED});
    if spec.blob {
        let path = format!("{}/r0/name", "/level".repeat(spec.depth));
        let old_len = serde_json::to_vec(&tree).unwrap().len();
        let length = spec
            .size
            .saturating_sub(old_len + if spec.changed_blob { 0 } else { 16 });
        let value = Value::String("x".repeat(length));
        if spec.changed_blob {
            *tree.pointer_mut(&path).unwrap() = value;
        } else {
            tree["unchanged"] = value;
        }
    } else {
        // Realistic small event objects dominate the unchanged subtree.
        let mut records = Vec::new();
        let base_len = serde_json::to_vec(&tree).unwrap().len();
        let mut length = base_len + 13;
        while length < spec.size.saturating_sub(110) {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let record = json!({"event": "terminal.output", "seq": records.len(), "value": seed % 100000,
                                "text": "hello terminal", "flags": [false, null]});
            length += serde_json::to_vec(&record).unwrap().len() + 1;
            records.push(record);
        }
        tree["unchanged"] = Value::Array(records);
        let length = serde_json::to_vec(&tree).unwrap().len();
        tree["padding"] = Value::String("x".repeat(spec.size.saturating_sub(length + 13)));
    }
    serde_json::to_vec(&tree).unwrap()
}

impl Case {
    pub fn new(spec: Spec) -> Self {
        let input = fixture(spec);
        let plan = registry(spec)
            .compile(Revision::V1, target(spec.hops))
            .unwrap();
        let clients = (0..spec.clients).map(|_| Arc::clone(&plan)).collect();
        Self {
            spec,
            input,
            plan,
            clients,
        }
    }

    pub fn verify(&self) {
        let input: Value = serde_json::from_slice(&self.input).unwrap();
        let expected = self.plan.translate(&input).unwrap();
        let stream = self
            .plan
            .translate_bytes(&self.input, &StreamingLimits::default())
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&stream.bytes).unwrap(),
            expected
        );
        assert!(self.input.len().abs_diff(self.spec.size) < 128);
        for client in &self.clients {
            assert!(Arc::ptr_eq(client, &self.plan));
        }
    }

    pub fn operation(&self, method: &str) {
        for plan in &self.clients {
            match method {
                "value_e2e" => {
                    let input: Value = serde_json::from_slice(black_box(&self.input)).unwrap();
                    let output = plan.translate(&input).unwrap();
                    black_box(serde_json::to_vec(&output).unwrap());
                }
                "stream_e2e" => {
                    black_box(
                        plan.translate_bytes(black_box(&self.input), &StreamingLimits::default())
                            .unwrap(),
                    );
                }
                _ => panic!("unknown method {method}"),
            }
        }
    }
}
