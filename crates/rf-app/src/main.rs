//! Rustfort executable.
fn main() {
    let w = rf_core::World::synthetic(48, 48, 20);
    println!("Rustfort skeleton: {} citizens in {}", w.citizens().count(), w.meta.fort_name);
}
