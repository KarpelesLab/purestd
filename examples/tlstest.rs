#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::thread;
use core::cell::Cell;

purestd::thread_local! {
    static COUNTER: Cell<u32> = Cell::new(0);
    static NAME: core::cell::RefCell<purestd::alloc::string::String> =
        core::cell::RefCell::new(purestd::alloc::string::String::from("main"));
}

fn main() {
    COUNTER.with(|c| c.set(100));
    NAME.with(|n| *n.borrow_mut() = "MAIN".into());

    let child = thread::spawn(|| {
        COUNTER.with(|c| c.set(7));          // child's own slot
        NAME.with(|n| *n.borrow_mut() = "child".into());
        let nm = NAME.with(|n| n.borrow().clone());
        (COUNTER.with(|c| c.get()), nm)
    }).join().unwrap();

    println!("main COUNTER = {} (expect 100)", COUNTER.with(|c| c.get()));
    println!("child returned = {:?} (expect (7, \"child\"))", child);
    println!("main NAME = {:?} (expect MAIN)", NAME.with(|n| n.borrow().clone()));
    println!("current tid = {:?} (nonzero)", thread::current().id());
    println!("tlstest: OK");
}
purestd::entry!(main);
