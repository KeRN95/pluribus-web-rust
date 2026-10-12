//! What the site's things are, apart from the pages that show them and the database that keeps
//! them: the kinds the schema's CHECKs spell out (a listing's status and kind, its price's kind,
//! its condition; a store's roles; why a listing waits for review; a property's data type), and a
//! notification's kind, as Rust enums (kinds.rs), with the rules about where a listing stands that
//! "Njoftimet e mia", the listings kept, the conversations and the seller's buttons share
//! (`Status`'s methods); the categories and the places (towns, their zones) listings go into
//! (categories.rs, re-exported as `domain::Category` and the rest, and by pluribus-db as
//! `db::Category`); a listing's own rules, what it must say to go up and what its price may be
//! (listing.rs, as `domain::listing::...`); what a notification says, from what its payload
//! keeps (notices.rs, `domain::notices::title`); and the rules about sending a message
//! (messaging.rs), which say why one can't go as a code the conversation's page words
//! (`messaging::Refusal`).

pub mod categories;
pub mod kinds;
pub mod listing;
pub mod messaging;
pub mod notices;

pub use categories::*;
pub use kinds::*;
