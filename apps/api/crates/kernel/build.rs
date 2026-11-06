// `sqlx::migrate!` embeds the migration files at compile time; without this, adding a new
// migration does not rebuild the crate and `app migrate` silently skips it.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
