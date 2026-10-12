//! The kinds the database spells out, as enums: each variant one value of its column, spelt as the
//! SQL has it (`as_str`, `Display`, serde) and read back from that spelling (`parse`, `FromStr`).
//! Rows keep their columns as text; the code that decides by one parses it where it does
//! (`Status::parse(&row.status)`), and a query is given `as_str()`, so the SQL and the queries
//! checked against .sqlx stay as they were. The rules about where a listing stands are `Status`'s
//! methods, which "Njoftimet e mia" (web/post.rs), the listings kept (web/saved.rs), the
//! conversations (web/messages.rs) and the seller's actions (db/posting.rs) share.
//!
//! The schema's CHECKs stay the database's own guard; the tests below read the migrations and hold
//! each enum to the CHECK that defines its column, so that a migration adding a value fails the
//! tests until its enum has it too. `NotificationKind`'s column has no CHECK: its test holds it to
//! the kinds the code writes.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{format, i18n::t};

/// A text that is none of a kind's values (`FromStr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownKind {
    /// The kind it was read as: "Status", "ListingType"...
    pub kind: &'static str,
}

impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a {} the database knows", self.kind)
    }
}

impl std::error::Error for UnknownKind {}

/// Declares a kind: an enum whose variants are its column's values, each with its SQL spelling,
/// and with `ALL` (every one, in the order declared), `NAMES` (their spellings, in that order),
/// `as_str`, `parse`, `names` (the spellings of a few), `Display` and `FromStr`. Serde spells them
/// in SCREAMING_SNAKE_CASE, as the SQL does (a test checks that it agrees with `as_str`).
macro_rules! kinds {
    (
        $(#[$meta:meta])*
        pub enum $kind:ident {
            $( $(#[$variant_meta:meta])* $variant:ident = $sql:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum $kind {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $kind {
            /// Every one, in the order declared.
            pub const ALL: [Self; [$($sql),+].len()] = [$(Self::$variant),+];

            /// Every one's spelling, in the same order.
            pub const NAMES: [&'static str; [$($sql),+].len()] = Self::names(Self::ALL);

            /// As the database spells it.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $sql, )+
                }
            }

            /// The one the database spells `text`; None for any other text.
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $( $sql => Some(Self::$variant), )+
                    _ => None,
                }
            }

            /// The spellings of `kinds`, in their order: a set a query is given, or a form offers.
            pub const fn names<const N: usize>(kinds: [Self; N]) -> [&'static str; N] {
                let mut names = [""; N];
                let mut at = 0;
                while at < N {
                    names[at] = kinds[at].as_str();
                    at += 1;
                }
                names
            }
        }

        impl fmt::Display for $kind {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $kind {
            type Err = UnknownKind;

            fn from_str(text: &str) -> Result<Self, UnknownKind> {
                Self::parse(text).ok_or(UnknownKind { kind: stringify!($kind) })
            }
        }
    };
}

kinds! {
    /// Where a listing stands (listings.status: migration 0001's CHECK, which 0011 gave PAUSED).
    /// The rules about it are the methods below.
    pub enum Status {
        /// Not up yet: kept until its seller confirms their contact with a code
        /// (db::create_listing's `publish`, sign_in.rs).
        Draft = "DRAFT",
        /// Waiting for a moderator before it goes up, why in its review_reason (ReviewReason).
        PendingReview = "PENDING_REVIEW",
        /// Up on the site.
        Active = "ACTIVE",
        /// Taken down for a while by its seller ("Çaktivizo"): only they still see it.
        Paused = "PAUSED",
        /// Up, and someone is taking it ("Rezervo").
        Reserved = "RESERVED",
        /// Sold, or let out ("U shit", "U dha me qira"): its page stays, saying so.
        Sold = "SOLD",
        /// Its time ran out (expiry.rs), until its seller renews it ("Rifresko").
        Expired = "EXPIRED",
        /// Refused by a moderator, waiting for them or after reports (db/review.rs, db/safety.rs).
        Rejected = "REJECTED",
        /// Taken off the site by its reports, or with its seller's account, until a moderator
        /// decides (db/safety.rs, db/review.rs).
        Suspended = "SUSPENDED",
        /// Deleted by its seller ("Fshi"): it shows nowhere.
        Deleted = "DELETED",
    }
}

kinds! {
    /// What a listing offers or asks for (listings.listing_type, migration 0001).
    pub enum ListingType {
        /// On sale ("Në shitje").
        Sell = "SELL",
        /// Someone wants to buy ("Kërkoj të blej").
        Buy = "BUY",
        /// For rent ("Me qira"), its price a month's.
        Rent = "RENT",
        /// Someone wants to rent ("Kërkoj me qira").
        WantedRent = "WANTED_RENT",
        /// A service ("Shërbim").
        Service = "SERVICE",
        /// A job ("Ofertë pune"), its price a month's pay.
        Job = "JOB",
    }
}

kinds! {
    /// How a listing's price is meant (listings.price_type, migration 0001).
    pub enum PriceType {
        /// The price, as it is ("Çmim fiks").
        Fixed = "FIXED",
        /// The price, open to an offer ("I diskutueshëm").
        Negotiable = "NEGOTIABLE",
        /// The price it starts from ("nga 925 €").
        From = "FROM",
        /// Nothing to pay ("Falas").
        Free = "FREE",
        /// No price given: agreed with the seller ("Me marrëveshje").
        OnRequest = "ON_REQUEST",
    }
}

kinds! {
    /// What state a thing is in (listings.condition, migration 0001), for the categories that ask.
    pub enum Condition {
        /// "E re".
        New = "NEW",
        /// "Si e re".
        LikeNew = "LIKE_NEW",
        /// "E përdorur".
        Used = "USED",
        /// "Për pjesë".
        ForParts = "FOR_PARTS",
    }
}

kinds! {
    /// What one of a store's people may do there (business_members.role, migration 0001).
    pub enum Role {
        /// The store's maker, one a store: everything, its people too ("Pronar").
        Owner = "OWNER",
        /// Changes the store and its people, posts its listings and answers its messages
        /// ("Administrator").
        Admin = "ADMIN",
        /// Posts and changes the store's listings, and answers its messages ("Poston njoftime").
        Editor = "EDITOR",
        /// Answers the store's messages, and nothing else ("Vetëm mesazhet").
        Viewer = "VIEWER",
    }
}

kinds! {
    /// Why a listing waits for a moderator before it goes up (listings.review_reason, migration
    /// 0037), on a site with moderators: web/post.rs decides, db/review.rs keeps the queue.
    pub enum ReviewReason {
        /// The first listing of an account no code has confirmed, posted without signing in; the
        /// code sent with it can put it up at once all the same.
        NewAccount = "NEW_ACCOUNT",
        /// A photo another seller's listing has too, the very same file (listing_media.sha256).
        DuplicatePhotos = "DUPLICATE_PHOTOS",
    }
}

kinds! {
    /// What a property's values are (attributes.data_type, migration 0001), and so which column of
    /// listing_attribute_values keeps them; a MULTISELECT's go to listing_attribute_options.
    pub enum DataType {
        /// Words, as typed (value_text).
        Text = "TEXT",
        /// A number (value_number), in its property's unit.
        Number = "NUMBER",
        /// Yes or no (value_boolean).
        Boolean = "BOOLEAN",
        /// A date (value_date).
        Date = "DATE",
        /// One of its options (value_option_id).
        Select = "SELECT",
        /// Any of its options (listing_attribute_options).
        Multiselect = "MULTISELECT",
    }
}

kinds! {
    /// What a notification is about (notifications.notification_type, which no CHECK limits: these
    /// are the kinds the code writes, where each says; web/notifications.rs shows them).
    pub enum NotificationKind {
        /// A listing the user keeps dropped its price (db/price_drops.rs).
        PriceDrop = "PRICE_DROP",
        /// A search the user saved has new listings (db/searches.rs).
        SavedSearch = "SAVED_SEARCH",
        /// A message waits unread (db/notices.rs).
        NewMessage = "NEW_MESSAGE",
        /// One of the user's listings expires soon (db/expiry.rs).
        Expiring = "EXPIRING",
        /// One of the user's listings expired (db/expiry.rs).
        Expired = "EXPIRED",
        /// A moderator promoted one of the user's listings (db/promotions.rs).
        Promoted = "PROMOTED",
        /// The user is asked into a store (db/stores.rs).
        StoreInvite = "STORE_INVITE",
    }
}

impl Status {
    /// Up on the site, in its lists and its searches: active, or reserved. Only such a listing can
    /// be reported (db/safety.rs), and only such a one expires or is said to expire soon.
    pub fn up(self) -> bool {
        matches!(self, Self::Active | Self::Reserved)
    }

    /// On the site for anyone with its link: up, or sold (its page stays, saying so); not taken off
    /// it by its reports or the moderators, nor down for a while by its seller, nor expired.
    pub fn on_site(self) -> bool {
        matches!(self, Self::Active | Self::Reserved | Self::Sold)
    }

    /// Up, or taken down by its seller for a while: it can be edited (db::own_listing).
    pub fn editable(self) -> bool {
        matches!(self, Self::Active | Self::Reserved | Self::Paused)
    }

    /// Up, so it can be taken down for a while ("Çaktivizo").
    pub fn pausable(self) -> bool {
        self.up()
    }

    /// Taken down by its seller, or sold, so it can be put back up ("Aktivizo").
    pub fn resumable(self) -> bool {
        matches!(self, Self::Paused | Self::Sold)
    }

    /// Up, and of a kind that can be handed over (ListingType::sellable), so it can be said to be
    /// sold ("U shit", "U dha me qira") or reserved.
    pub fn sellable(self, kind: ListingType) -> bool {
        self.up() && kind.sellable()
    }

    /// Expired, or expiring within a week (`expires_soon`, as format::expires_soon says of a listing
    /// up): "Rifresko" keeps it up db::LISTING_DAYS more.
    pub fn renewable(self, expires_soon: bool) -> bool {
        self == Self::Expired || expires_soon
    }

    /// What its row on "Njoftimet e mia" says of it, once it is no longer simply up; a sold one by
    /// its kind ("Shitur", "Dhënë me qira" for a rental: format::sold).
    pub fn seller_label(self, kind: ListingType) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Reserved => Some(t("Rezervuar")),
            Self::Sold => Some(format::sold(kind.as_str())),
            Self::Expired => Some(t("Skaduar")),
            Self::PendingReview | Self::Draft => Some(t("Në pritje")),
            Self::Paused => Some(t("Çaktivizuar")),
            Self::Suspended => Some(t("Në shqyrtim")),
            Self::Rejected => Some(t("Hequr nga moderatorët")),
            Self::Deleted => Some(t("Jo aktiv")),
        }
    }

    /// Why a listing someone keeps isn't on the site, as its row on /te-ruajturat says.
    pub fn gone_label(self) -> &'static str {
        match self {
            Self::Paused => t("Çaktivizuar nga shitësi"),
            Self::Deleted => t("U fshi"),
            Self::Expired => t("Skadoi"),
            Self::Suspended | Self::Rejected => t("Hequr nga faqja"),
            Self::Draft | Self::PendingReview | Self::Active | Self::Reserved | Self::Sold => t("Jo në faqe"),
        }
    }

    /// What a conversation's head says of its listing, once it is no longer simply up, so that
    /// neither side writes on about a thing gone: reserved, sold ("Dhënë me qira" for a rental, by
    /// its kind) or off the site.
    pub fn conversation_label(self, kind: ListingType) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Reserved => Some(t("Rezervuar")),
            Self::Sold => Some(format::sold(kind.as_str())),
            Self::Draft
            | Self::PendingReview
            | Self::Paused
            | Self::Expired
            | Self::Rejected
            | Self::Suspended
            | Self::Deleted => Some(t("Jo më në faqe")),
        }
    }

    /// The tone its badge takes (.badge.warn, .badge.danger in listing.css), as the canvas has them:
    /// a reservation in the warm yellow, a listing expired or taken off in red's tint; the rest
    /// grey (waiting for review, paused, sold, deleted).
    pub fn tone(self) -> Option<&'static str> {
        match self {
            Self::Reserved => Some("warn"),
            Self::Expired | Self::Suspended | Self::Rejected => Some("danger"),
            Self::Draft | Self::PendingReview | Self::Active | Self::Paused | Self::Sold | Self::Deleted => None,
        }
    }
}

impl ListingType {
    /// The kinds that can be sold or reserved: those on sale and those for rent (a wanted ad, a
    /// job or a service has nothing to hand over).
    pub const SELLABLE: [Self; 2] = [Self::Sell, Self::Rent];

    /// One of SELLABLE.
    pub fn sellable(self) -> bool {
        Self::SELLABLE.contains(&self)
    }
}

impl Role {
    /// The roles a store's people are given, the owner's aside (business_invitations.role,
    /// migration 0035): who changes the store and its people, who posts its listings, and who
    /// answers its messages alone.
    pub const GIVEN: [Self; 3] = [Self::Admin, Self::Editor, Self::Viewer];

    /// Whether one of this role may post the store's listings, and change them (manages_listing,
    /// migration 0035): its owner, its admins and its editors.
    pub fn posts(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Editor)
    }

    /// Whether one of this role may change the store itself, and its people: its owner and its
    /// admins.
    pub fn manages(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// What the role is called, as the store's pages and an invitation say it, in the language now.
    pub fn label(self) -> &'static str {
        match self {
            Self::Owner => t("Pronar"),
            Self::Admin => t("Administrator"),
            Self::Editor => t("Poston njoftime"),
            Self::Viewer => t("Vetëm mesazhet"),
        }
    }

    /// What the role lets one of the store's people do, as the invitation form and an invitation
    /// say it, in the language now.
    pub fn does(self) -> &'static str {
        match self {
            Self::Owner => t("Gjithçka, edhe njerëzit e dyqanit."),
            Self::Admin => t("Ndryshon dyqanin dhe njerëzit e tij, poston njoftime dhe u përgjigjet mesazheve."),
            Self::Editor => t("Poston dhe ndryshon njoftimet e dyqanit, dhe u përgjigjet mesazheve."),
            Self::Viewer => t("U përgjigjet mesazheve të dyqanit."),
        }
    }
}

impl NotificationKind {
    /// The kinds about the user's own listings, which the account button's dot stands for: the
    /// others have the heart's dot (prices dropped, saved searches) and the conversations' count
    /// (messages).
    pub const OWN_LISTING: [Self; 3] = [Self::Expiring, Self::Expired, Self::Promoted];
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::PathBuf, str::FromStr};

    use serde::de::DeserializeOwned;

    use super::*;

    /// The values the migrations' CHECKs let `table`.`column` take, as the last of them to say so
    /// has it: the column's own CHECK where its table is made or where it is added (ADD COLUMN), or
    /// its constraint made again (ADD CONSTRAINT {table}_{column}_check, as 0011 did for
    /// listings.status).
    fn checked(table: &str, column: &str) -> BTreeSet<String> {
        let mut migrations: Vec<PathBuf> = fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        migrations.sort();
        let mut last = None;
        for migration in migrations {
            let sql = fs::read_to_string(&migration).unwrap();
            let mut said = Vec::new();
            // Where its table is made: the column's line, up to the next column's.
            if let Some(start) = sql.find(&format!("CREATE TABLE {table} (")) {
                let table_sql = &sql[start..];
                let table_sql = &table_sql[..table_sql.find("\n);").unwrap_or(table_sql.len())];
                if let Some(line) = table_sql.find(&format!("\n    {column} ")) {
                    let definition = &table_sql[line + 1..];
                    let next = definition
                        .match_indices("\n    ")
                        .find(|(at, _)| !definition[at + 5..].starts_with(' '))
                        .map_or(definition.len(), |(at, _)| at);
                    said.extend(values_in(&definition[..next], column).map(|values| (start + line, values)));
                }
            }
            for opening in [format!("ALTER TABLE {table} ADD COLUMN {column} "), format!("ADD CONSTRAINT {table}_{column}_check")] {
                for (at, _) in sql.match_indices(&opening) {
                    let statement = &sql[at..];
                    let statement = &statement[..statement.find(';').unwrap_or(statement.len())];
                    said.extend(values_in(statement, column).map(|values| (at, values)));
                }
            }
            if let Some((_, values)) = said.into_iter().max_by_key(|(at, _)| *at) {
                last = Some(values);
            }
        }
        last.unwrap_or_else(|| panic!("no CHECK in migrations/ says what {table}.{column} takes"))
    }

    /// The quoted values of the first "{column} IN (…)" in `sql`.
    fn values_in(sql: &str, column: &str) -> Option<BTreeSet<String>> {
        let opening = format!("{column} IN (");
        let start = sql.find(&opening)? + opening.len();
        let list = &sql[start..start + sql[start..].find(')')?];
        Some(list.split('\'').skip(1).step_by(2).map(str::to_string).collect())
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn each_kind_has_the_values_its_check_allows() {
        let kinds = [
            ("Status", set(&Status::NAMES), checked("listings", "status")),
            ("ListingType", set(&ListingType::NAMES), checked("listings", "listing_type")),
            ("PriceType", set(&PriceType::NAMES), checked("listings", "price_type")),
            ("Condition", set(&Condition::NAMES), checked("listings", "condition")),
            ("Role", set(&Role::NAMES), checked("business_members", "role")),
            ("Role::GIVEN", set(&Role::names(Role::GIVEN)), checked("business_invitations", "role")),
            ("ReviewReason", set(&ReviewReason::NAMES), checked("listings", "review_reason")),
            ("DataType", set(&DataType::NAMES), checked("attributes", "data_type")),
        ];
        for (kind, declared, allowed) in kinds {
            assert_eq!(declared, allowed, "{kind} (crates/core/src/domain/kinds.rs) has other values than its column's CHECK in migrations/");
        }
    }

    /// The kinds the code's INSERTs into notifications write: the first quoted upper-case value
    /// after each, in the SQL's own string, in every crate's sources and the binary's.
    fn notification_kinds_written() -> BTreeSet<String> {
        // Spelt in two, so that this file isn't found to write one.
        let insert = concat!("INSERT INTO ", "notifications (");
        let mut kinds = BTreeSet::new();
        let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let mut stack: Vec<PathBuf> = fs::read_dir(root.join("crates")).unwrap().map(|entry| entry.unwrap().path().join("src")).collect();
        stack.push(root.join("src"));
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(fs::read_dir(&path).unwrap().map(|entry| entry.unwrap().path()));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = fs::read_to_string(&path).unwrap();
                for (at, _) in source.match_indices(insert) {
                    let statement = &source[at + insert.len()..];
                    let statement = &statement[..statement.find('"').unwrap_or(statement.len())];
                    let kind = statement
                        .split('\'')
                        .skip(1)
                        .step_by(2)
                        .find(|value| !value.is_empty() && value.chars().all(|c| c.is_ascii_uppercase() || c == '_'));
                    kinds.extend(kind.map(str::to_string));
                }
            }
        }
        kinds
    }

    #[test]
    fn notification_kinds_are_those_the_code_writes() {
        assert_eq!(
            set(&NotificationKind::NAMES),
            notification_kinds_written(),
            "NotificationKind (crates/core/src/domain/kinds.rs) and the kinds the crates write into notifications differ"
        );
    }

    /// Each one of a kind spelt alike everywhere: `as_str`, `Display` and serde, read back by
    /// `parse`, `FromStr` and serde; a text that is none of them reads as none.
    fn spelt_alike<K>(all: &[K], as_str: fn(K) -> &'static str, parse: fn(&str) -> Option<K>)
    where
        K: Copy + PartialEq + fmt::Debug + fmt::Display + FromStr + Serialize + DeserializeOwned,
    {
        for &kind in all {
            let text = as_str(kind);
            assert_eq!(parse(text), Some(kind));
            assert_eq!(text.parse::<K>().ok(), Some(kind));
            assert_eq!(kind.to_string(), text);
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<K>(&format!("\"{text}\"")).unwrap(), kind);
            assert_eq!(parse(&text.to_lowercase()), None, "{text} only as the SQL spells it");
        }
        assert_eq!(parse(""), None);
        assert!("NOTHING".parse::<K>().is_err());
    }

    #[test]
    fn each_kind_is_spelt_as_the_sql_spells_it() {
        spelt_alike(&Status::ALL, Status::as_str, Status::parse);
        spelt_alike(&ListingType::ALL, ListingType::as_str, ListingType::parse);
        spelt_alike(&PriceType::ALL, PriceType::as_str, PriceType::parse);
        spelt_alike(&Condition::ALL, Condition::as_str, Condition::parse);
        spelt_alike(&Role::ALL, Role::as_str, Role::parse);
        spelt_alike(&ReviewReason::ALL, ReviewReason::as_str, ReviewReason::parse);
        spelt_alike(&DataType::ALL, DataType::as_str, DataType::parse);
        spelt_alike(&NotificationKind::ALL, NotificationKind::as_str, NotificationKind::parse);
        assert_eq!("PAUSED".parse::<Status>(), Ok(Status::Paused));
        assert_eq!("PAUSE".parse::<Status>(), Err(UnknownKind { kind: "Status" }));
    }
}
