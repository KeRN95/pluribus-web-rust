// Rebuild when a migration changes, so sqlx::migrate!() embeds the current files.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
