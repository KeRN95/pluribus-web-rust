// Rebuild when a migration changes, so that sqlx::migrate!() (MIGRATOR, lib.rs) embeds the current
// files.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
