mod handler;

use crate::handler::Handler;

fn main() {
    let handler = Handler::new();
    handler.handle();
    process();
}

fn process() {
    println!("processing");
}
