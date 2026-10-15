rustx::sugar! {
    mod bus {
        use std::collections::HashMap;

        pub struct Event {
            pub topic: String,
            pub payload: String,
        }

        pub trait Sink {
            fn deliver(&self, e: &Event);
        }

        pub struct Bus {
            handlers: HashMap<String, Vec<~Fn(&Event) -> bool>>,
            sinks: Vec<~Sink>,
        }

        impl Bus {
            pub fn new() -> Self {
                Bus { handlers: HashMap::new(), sinks: Vec::new() }
            }

            pub fn on(&mut self, topic: &str, h: ~Fn(&Event) -> bool) {
                self.handlers.entry(topic.to_string()).or_default().push(h);
            }

            pub fn add_sink(&mut self, s: ~Sink) {
                self.sinks.push(s);
            }

            pub fn publish(&self, e: Event) -> usize {
                let mut handled = 0;
                if let Some(hs) = self.handlers.get(&e.topic) {
                    for h in hs {
                        if h(&e) { handled += 1; }
                    }
                }
                for s in &self.sinks { s.deliver(&e); }
                handled
            }
        }
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Counter(Arc<AtomicUsize>);

impl bus::Sink for Counter {
    fn deliver(&self, e: &bus::Event) {
        self.0.fetch_add(e.payload.len(), Ordering::SeqCst);
    }
}

fn main() {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut b = bus::Bus::new();
    b.on("a", Box::new(|e| e.payload == "hi"));
    b.on("a", Box::new(|_| false));
    b.add_sink(Box::new(Counter(Arc::clone(&seen))));
    let handled = b.publish(bus::Event { topic: "a".into(), payload: "hi".into() });
    assert_eq!(handled, 1);
    assert_eq!(seen.load(Ordering::SeqCst), 2);
    // The sugar promises `Send + Sync`, so the bus can cross threads.
    std::thread::spawn(move || drop(b)).join().unwrap();
}
