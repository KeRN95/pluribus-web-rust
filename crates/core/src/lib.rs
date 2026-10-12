//! What the site's things are and how it says them, apart from the pages that show them
//! (pluribus-web) and the database that keeps them (pluribus-db): the domain (`domain`: the kinds
//! the schema's CHECKs spell out, the categories and places listings go into, a listing's own
//! rules, what a notification says, the rules about sending a message), the site in two languages
//! (`i18n`: `t` and its kin, with the English catalogue build.rs makes a table of), and how it
//! words prices, counts, dates, phone numbers and its addresses (`format`). No sqlx, no axum: the
//! other crates build on it, and it on none of them.

pub mod domain;
pub mod format;
pub mod i18n;

pub use i18n::{t, t1, t2, t3, tf};
