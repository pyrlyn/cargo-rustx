//! An event bus with boxed handlers and borrowing iterators, written without sugar.

use std::collections::HashMap;
use std::fmt::Debug;

pub trait Sink {
    fn write(&self, line: &str);
}

pub struct Bus {
    pub sinks: Vec<Box<dyn Sink + Send + Sync>>,
    // Handlers run on any thread, so they must be `Send + Sync`.
    pub handlers: HashMap<String, Vec<Box<dyn Fn(&str) -> bool + Send + Sync>>>,
    pub tagged: Vec<(String, Box<dyn Sink + Send + Sync>)>,
}

pub type Logger = Box<dyn Sink + Debug + Send + Sync>;

impl Bus {
    pub fn add_sink(&mut self, sink: Box<dyn Sink + Send + Sync>) {
        self.sinks.push(sink);
    }

    pub fn topics(&self) -> impl Iterator<Item = &String> + '_ {
        self.handlers.keys()
    }

    pub fn handlers_of(&self, topic: &str) -> impl Iterator<Item = (usize, &Box<dyn Fn(&str) -> bool + Send + Sync>)> + '_ {
        self.handlers.get(topic).into_iter().flatten().enumerate()
    }

    // A box without the markers is not what `~` writes, so it keeps its spelling.
    pub fn first(&self) -> Option<&Box<dyn Sink>> {
        None
    }

    // Without `+ '_` this is not what `iter` writes.
    pub fn counts(&self) -> impl Iterator<Item = usize> {
        vec![self.sinks.len()].into_iter()
    }
}
