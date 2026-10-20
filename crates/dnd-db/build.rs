// `sqlx::migrate!` embeds migrations at compile time, but cargo does not watch that folder on its
// own: without this, a new migration file would not trigger a rebuild.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
