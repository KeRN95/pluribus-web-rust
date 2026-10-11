//! The icons of a listing's details (listing.html): one before each property's value, drawn as
//! the welcome page's categories are (browse.rs), in strokes in a 24-unit box, each the inside of
//! an SVG. Where the category says more than the property, the icon is the category's: a phone's
//! brand is drawn as a phone, a dog's breed as a dog (IN_CATEGORIES). Beside its icon a value
//! reads alone where it says what it is ("Euro 6", "128 GB"), and after the property's name where
//! it doesn't ("Kati 6"); a box ticked shows its name alone ("Pa aksident"). The posting form's
//! tiles take a car's (properties.rs).

use self::Reads::{Alone, Named};

/// How a property's value reads beside its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reads {
    /// Alone: it says what it is ("Euro 6", "1+1", "Naftë").
    Alone,
    /// After the property's name, as it does without an icon ("Kati 6", "Tërheqja Përpara").
    Named,
}

/// A property's icon on a listing's page, and how its value reads beside it.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub icon: &'static str,
    pub reads: Reads,
}

/// The look of a property, by its code, on the page of a listing in `categories`: the listing's
/// category's slug first, then those of the categories above it. None for a property without
/// an icon: a multi-select's values each have a check instead (listing.html).
pub fn look(categories: &[&str], code: &str) -> Option<Look> {
    let in_category = categories.iter().find_map(|slug| {
        IN_CATEGORIES.iter().find(|(category, of, ..)| category == slug && *of == code)
    });
    match in_category {
        Some(&(_, _, icon, reads)) => Some(Look { icon, reads }),
        None => PROPERTIES.iter().find(|(of, ..)| *of == code).map(|&(_, icon, reads)| Look { icon, reads }),
    }
}


/// Each property's icon, and how its value reads beside it, by code.
const PROPERTIES: [(&str, &str, Reads); 144] = [
    ("appliance_brand", BRAND, Alone),
    ("appliance_type", WASHING_MACHINE, Alone),
    ("audio_noise_cancelling", MUTE, Named),
    ("audio_wireless", BLUETOOTH, Named),
    ("bike_gears", COG, Named),
    ("bike_type", BICYCLE, Alone),
    ("camera_mount", LENS, Alone),
    ("camera_sensor", SENSOR, Alone),
    ("camera_shutter_count", SHUTTER, Named),
    ("camping_type", TENT, Alone),
    ("capacity", JUG, Named),
    ("charger_connector", PLUG, Alone),
    ("commercial_make", TRUCK, Alone),
    ("commercial_type", VAN, Alone),
    ("computer_cpu", CHIP, Alone),
    ("computer_disk", DRIVE, Alone),
    ("computer_gpu", GRAPHICS_CARD, Alone),
    ("computer_monitor", SCREEN, Named),
    ("computer_os", APPS, Alone),
    ("console_controllers", JOYSTICK, Named),
    ("console_edition", NAME_TAG, Named),
    ("console_games", DISC, Named),
    ("console_model", GAMEPAD, Alone),
    ("decor_type", LAMP, Alone),
    ("dimensions", RULER, Alone),
    ("electronics_brand", BRAND, Alone),
    ("electronics_color", PALETTE, Alone),
    ("electronics_device", TOWER, Alone),
    ("electronics_ram", RAM, Alone),
    ("electronics_screen", DIAGONAL, Alone),
    ("electronics_storage", MEMORY_CARD, Alone),
    ("electronics_warranty", SHIELD, Named),
    ("energy_class", ENERGY, Alone),
    ("fashion_brand", BRAND, Alone),
    ("fashion_for", PERSON, Alone),
    ("fashion_size", TAPE_MEASURE, Named),
    ("fitness_type", DUMBBELL, Alone),
    ("fits_from_year", CALENDAR, Named),
    ("fits_to_year", CALENDAR, Named),
    ("frame_size", RULER, Named),
    ("furniture_type", CHAIR, Alone),
    ("garden_type", SPROUT, Alone),
    ("instrument", NOTES, Alone),
    ("instrument_kind", SOUND_WAVE, Alone),
    ("item_color", PALETTE, Alone),
    ("job_accommodation", BED, Named),
    ("job_contract", DOCUMENT, Named),
    ("job_education", MORTARBOARD, Named),
    ("job_experience", STAR, Named),
    ("job_meals", FORK_AND_KNIFE, Named),
    ("job_place", OFFICE, Alone),
    ("job_schedule", CLOCK, Alone),
    ("job_shift", SHIFTS, Named),
    ("kids_size", TAPE_MEASURE, Alone),
    ("land_permit", PERMIT, Named),
    ("land_road", ROAD, Named),
    ("land_type", HILLS, Alone),
    ("length_cm", RULER, Alone),
    ("lesson_mode", LAPTOP, Alone),
    ("lesson_subject", OPEN_BOOK, Alone),
    ("machine_brand", BRAND, Alone),
    ("machine_hours", STOPWATCH, Alone),
    ("machine_kind", COG, Alone),
    ("machine_weight", WEIGHT, Alone),
    ("machine_year", CALENDAR, Alone),
    ("material", TEXTURE, Alone),
    ("model_name", NAME_TAG, Alone),
    ("moto_license", ID_CARD, Named),
    ("moto_make", MOTORBIKE, Alone),
    ("moto_type", HELMET, Alone),
    ("office_rooms", DOOR, Named),
    ("parking_spaces", PARKING, Named),
    ("part_number", HASH, Alone),
    ("payload", WEIGHT, Named),
    ("pet_accessory", BONE, Alone),
    ("pet_age", CAKE, Alone),
    ("pet_breed", PAW, Alone),
    ("pet_for", PAW, Named),
    ("pet_microchip", CHIP, Named),
    ("pet_neutered", MEDICAL_CROSS, Named),
    ("pet_pedigree", ROSETTE, Named),
    ("pet_sex", SEXES, Alone),
    ("pet_vaccinated", SYRINGE, Named),
    ("phase", SINE_WAVE, Alone),
    ("phone_battery", BATTERY, Alone),
    ("phone_model", NAME_TAG, Alone),
    ("phone_unlocked", OPEN_LOCK, Named),
    ("power_kw", BOLT, Alone),
    ("property_area", AREA, Alone),
    ("property_bathrooms", BATH, Named),
    ("property_condition", SPARKLES, Named),
    ("property_floor", STAIRS, Named),
    ("property_frontage", WIDTH, Named),
    ("property_furnished", SOFA, Alone),
    ("property_heating", RADIATOR, Named),
    ("property_plot_area", FENCE, Named),
    ("property_rooms", BED, Alone),
    ("property_total_floors", BUILDING, Named),
    ("property_year_built", CALENDAR, Named),
    ("quantity", LAYERS, Alone),
    ("rent_deposit", BANKNOTE, Named),
    ("rent_pets", PAW, Named),
    ("rent_students", MORTARBOARD, Named),
    ("rent_utilities", LIGHT_BULB, Named),
    ("rim_size", DIAMETER, Alone),
    ("service_areas", MAP, Named),
    ("service_at_home", HOUSE, Named),
    ("service_experience", STAR, Named),
    ("service_invoice", RECEIPT, Named),
    ("shoe_size", FOOTPRINTS, Named),
    ("sport_brand", BRAND, Alone),
    ("tablet_connectivity", WIFI, Alone),
    ("tire_kind", TYRE, Alone),
    ("tire_profile", HEIGHT, Named),
    ("tire_season", THERMOMETER, Alone),
    ("tire_width", WIDTH, Named),
    ("transport_vehicle", BUS, Alone),
    ("tv_panel", LAYERS, Alone),
    ("tv_resolution", PIXELS, Alone),
    ("tv_smart", WIFI, Named),
    ("vehicle_body_type", CAR_SIDE, Alone),
    ("vehicle_color", PALETTE, Alone),
    ("vehicle_customs", STAMP, Alone),
    ("vehicle_doors", CAR_DOOR, Alone),
    ("vehicle_drive", AXLES, Named),
    ("vehicle_emission", LEAF, Alone),
    ("vehicle_engine", ENGINE, Alone),
    ("vehicle_fuel", FUEL_PUMP, Alone),
    ("vehicle_inspection", CLIPBOARD, Named),
    ("vehicle_insurance", UMBRELLA, Named),
    ("vehicle_make", CAR_FRONT, Alone),
    ("vehicle_mileage", SPEEDOMETER, Alone),
    ("vehicle_model", NAME_TAG, Alone),
    ("vehicle_no_accident", SHIELD, Named),
    ("vehicle_owners", PEOPLE, Named),
    ("vehicle_plates", NUMBER_PLATE, Alone),
    ("vehicle_power", BOLT, Alone),
    ("vehicle_seats", SEAT, Named),
    ("vehicle_service_book", BOOK, Named),
    ("vehicle_transmission", GEAR_LEVER, Alone),
    ("vehicle_year", CALENDAR, Alone),
    ("watch_movement", COG, Alone),
    ("wheel_size", WHEEL, Alone),
    ("winter_type", SNOWFLAKE, Alone),
];

/// A property's look in a category, and in those under it, where another fits it better than its
/// own: by the category's slug.
const IN_CATEGORIES: [(&str, &str, &str, Reads); 21] = [
    // A brand, where the category sells one kind of thing and nothing else on its page draws it:
    // a phone's, a jacket's.
    ("telefona", "electronics_brand", PHONE, Alone),
    ("laptope", "electronics_brand", LAPTOP, Alone),
    ("tableta", "electronics_brand", TABLET, Alone),
    ("televizore", "electronics_brand", TV, Alone),
    ("xhaketa-dhe-pallto", "fashion_brand", JACKET, Alone),
    ("kepuce", "fashion_brand", TRAINER, Alone),
    ("fustane", "fashion_brand", DRESS, Alone),
    ("canta", "fashion_brand", HANDBAG, Alone),
    ("ora", "fashion_brand", WRISTWATCH, Alone),
    ("aksesore", "fashion_brand", SUNGLASSES, Alone),
    // What the thing is, where the category has kinds: a camera, headphones; a desktop's is a
    // tower (PROPERTIES).
    ("aparate-fotografike", "electronics_device", CAMERA, Alone),
    ("audio-dhe-kufje", "electronics_device", HEADPHONES, Alone),
    ("makineri-ndertimi", "machine_kind", EXCAVATOR, Alone),
    ("makineri-bujqesore", "machine_kind", TRACTOR, Alone),
    ("energji-dhe-gjeneratore", "machine_kind", SOLAR_PANEL, Alone),
    // A pet's breed: the animal.
    ("qen", "pet_breed", DOG, Alone),
    ("mace", "pet_breed", CAT, Alone),
    ("zogj", "pet_breed", BIRD, Alone),
    ("peshq", "pet_breed", FISH, Alone),
    // A part's make and model are the vehicle's it fits, which their names say ("Marka e
    // automjetit"): on its own, "Volkswagen" would read as the part's maker.
    ("pjese-dhe-aksesore", "vehicle_make", CAR_FRONT, Named),
    ("pjese-dhe-aksesore", "vehicle_model", NAME_TAG, Named),
];

// The drawings, the car's first: the posting form's tiles take them too (properties.rs).

/// A car from the front.
pub(super) const CAR_FRONT: &str = r#"<path d="M5 11l1.5-4a2 2 0 0 1 1.9-1.3h7.2a2 2 0 0 1 1.9 1.3L19 11"/><rect x="3" y="11" width="18" height="6.5" rx="2"/><path d="M6.5 17.5V20M17.5 17.5V20"/><circle cx="7.5" cy="14.2" r=".9"/><circle cx="16.5" cy="14.2" r=".9"/>"#;

/// A name tag.
pub(super) const NAME_TAG: &str = r#"<path d="M3.5 12.4V5A1.5 1.5 0 0 1 5 3.5h7.4a1.5 1.5 0 0 1 1.06.44l7 7a1.5 1.5 0 0 1 0 2.12l-7.4 7.4a1.5 1.5 0 0 1-2.12 0l-7-7a1.5 1.5 0 0 1-.44-1.06Z"/><circle cx="8.5" cy="8.5" r="1.3"/>"#;

/// A calendar.
pub(super) const CALENDAR: &str = r#"<rect x="3.5" y="5" width="17" height="15.5" rx="2.5"/><path d="M3.5 10h17M8 3v4M16 3v4"/>"#;

/// A fuel pump.
pub(super) const FUEL_PUMP: &str = r#"<path d="M4.5 20.5v-15a2 2 0 0 1 2-2h5.5a2 2 0 0 1 2 2v15M3 20.5h12.5"/><rect x="7" y="6.5" width="4.5" height="3.5" rx=".8"/><path d="M14 11h1.5a1.5 1.5 0 0 1 1.5 1.5V16a1.5 1.5 0 0 0 3 0V8.5l-2.5-3"/>"#;

/// A gear lever's H.
pub(super) const GEAR_LEVER: &str = r#"<circle cx="6" cy="5.5" r="1.5"/><circle cx="12" cy="5.5" r="1.5"/><circle cx="18" cy="5.5" r="1.5"/><circle cx="6" cy="18.5" r="1.5"/><circle cx="12" cy="18.5" r="1.5"/><path d="M6 7v10M12 7v10M18 7v5H6"/>"#;

/// A car from the side.
pub(super) const CAR_SIDE: &str = r#"<path d="M5 16.5H3.5V13a1 1 0 0 1 .62-.92L6.5 11l2.2-3.3A1.5 1.5 0 0 1 9.95 7h4.7a1.5 1.5 0 0 1 1.17.56L18.5 11l1.4.42a1 1 0 0 1 .6.92v4.16H19M9 16.5h6"/><circle cx="7" cy="16.5" r="2"/><circle cx="17" cy="16.5" r="2"/>"#;

/// A speedometer.
pub(super) const SPEEDOMETER: &str = r#"<path d="M4.2 17.5a9 9 0 1 1 15.6 0"/><path d="m12 14 3.5-4"/><circle cx="12" cy="14.5" r="1.4"/>"#;

/// A paint palette.
pub(super) const PALETTE: &str = r#"<path d="M12 3a9 9 0 0 0 0 18c1.1 0 1.8-.9 1.8-1.8 0-.5-.2-.9-.5-1.2s-.5-.7-.5-1.2c0-1 .8-1.8 1.8-1.8h2.1a4.3 4.3 0 0 0 4.3-4.3C21 6.6 17 3 12 3Z"/><circle cx="7.5" cy="11" r="1"/><circle cx="10" cy="7" r="1"/><circle cx="14.5" cy="7" r="1"/>"#;

/// A map pin.
pub(super) const PIN: &str = r#"<path d="M12 21s-6.5-5.6-6.5-11a6.5 6.5 0 0 1 13 0c0 5.4-6.5 11-6.5 11Z"/><circle cx="12" cy="10" r="2.3"/>"#;

/// A house.
const HOUSE: &str = r#"<path d="M3.5 10.5 12 3.5l8.5 7"/><path d="M5.5 9v10.5a1 1 0 0 0 1 1H10v-5.5h4v5.5h3.5a1 1 0 0 0 1-1V9"/>"#;

/// A sofa.
const SOFA: &str = r#"<path d="M5.5 11V8.5A2.5 2.5 0 0 1 8 6h8a2.5 2.5 0 0 1 2.5 2.5V11"/><path d="M3.5 17.5v-5a2 2 0 0 1 4 0V14h9v-1.5a2 2 0 0 1 4 0v5z"/><path d="M5.5 17.5V20M18.5 17.5V20"/>"#;

/// A paw.
const PAW: &str = r#"<circle cx="8.5" cy="5.5" r="1.8"/><circle cx="15.5" cy="5.5" r="1.8"/><circle cx="4.8" cy="10.5" r="1.7"/><circle cx="19.2" cy="10.5" r="1.7"/><path d="M12 10.5c-2.6 0-5 3.2-5 5.8 0 1.6 1.2 2.7 2.6 2.7 1 0 1.6-.5 2.4-.5s1.4.5 2.4.5c1.4 0 2.6-1.1 2.6-2.7 0-2.6-2.4-5.8-5-5.8z"/>"#;

/// A motorbike.
const MOTORBIKE: &str = r#"<circle cx="5.5" cy="16.5" r="3"/><circle cx="18.5" cy="16.5" r="3"/><path d="M3.5 11.5h5.5l1.8-2h3.2"/><path d="M8 11.5l2.5 5h3l2.5-4.5"/><path d="M15.5 9.5 18.5 16.5"/><path d="M14.5 9.5l-1-2.5h2.5"/>"#;

/// A delivery truck.
const TRUCK: &str = r#"<path d="M14 16.5V6.5a1 1 0 0 0-1-1H3.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h1.3"/><path d="M14 9h3.8l3.7 4v2.5a1 1 0 0 1-1 1h-1.3"/><path d="M9.2 16.5h5.6"/><circle cx="7" cy="16.8" r="2.2"/><circle cx="17" cy="16.8" r="2.2"/>"#;

/// A tyre on its rim.
const TYRE: &str = r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="5"/><path d="M12 10.2V7M13.7 11.4l3.1-.9M13.1 13.5l1.8 2.5M10.9 13.5 9.1 16M10.3 11.4l-3.1-.9"/>"#;

/// A lightning bolt.
const BOLT: &str = r#"<path d="M13 2.5 5.5 13h6l-1 8.5L18.5 11h-6z"/>"#;

/// A bed.
const BED: &str = r#"<path d="M2.5 19V6"/><path d="M21.5 19v-5H2.5"/><path d="M2.5 14v-3.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1V14"/><path d="M8.5 11h11a2 2 0 0 1 2 2"/>"#;

/// An office tower.
const OFFICE: &str = r#"<rect x="5" y="2.5" width="14" height="19" rx="1"/><path d="M9 6.5h2M13 6.5h2M9 10.5h2M13 10.5h2M9 14.5h2M13 14.5h2"/><path d="M10.5 21.5v-3h3v3"/>"#;

/// A phone.
const PHONE: &str = r#"<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M10.5 18h3"/><circle cx="12" cy="5.5" r=".6"/>"#;

/// A laptop.
const LAPTOP: &str = r#"<rect x="4.5" y="4.5" width="15" height="10.5" rx="1.5"/><path d="M2.5 19.5h19"/><path d="M4.5 15 2.5 19.5M19.5 15l2 4.5"/>"#;

/// A screen on its stand.
const SCREEN: &str = r#"<rect x="3" y="4" width="18" height="12" rx="1.5"/><path d="M9 20h6M12 16v4"/>"#;

/// A tablet.
const TABLET: &str = r#"<rect x="4.5" y="2.5" width="15" height="19" rx="2.5"/><path d="M11 18h2"/>"#;

/// A television and its aerial.
const TV: &str = r#"<rect x="2.5" y="7" width="19" height="13" rx="2"/><path d="m8 3 4 4 4-4"/>"#;

/// A game controller.
const GAMEPAD: &str = r#"<path d="M6.5 7.5h11a4 4 0 0 1 4 4.4l-.6 4.8a2.5 2.5 0 0 1-4.3 1.4L14 15.5h-4l-2.6 2.6a2.5 2.5 0 0 1-4.3-1.4l-.6-4.8a4 4 0 0 1 4-4.4z"/><path d="M7.5 10v4M5.5 12h4"/><circle cx="16" cy="11" r=".6"/><circle cx="17.5" cy="13" r=".6"/>"#;

/// A camera.
const CAMERA: &str = r#"<path d="M3 8.5a2 2 0 0 1 2-2h2.5L9 4.5h6l1.5 2H19a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><circle cx="12" cy="13" r="3.5"/>"#;

/// Headphones.
const HEADPHONES: &str = r#"<path d="M4 14v-2a8 8 0 0 1 16 0v2"/><rect x="2.5" y="13.5" width="4.5" height="7" rx="1.5"/><rect x="17" y="13.5" width="4.5" height="7" rx="1.5"/>"#;

/// A chair.
const CHAIR: &str = r#"<path d="M7 11V5a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v6"/><path d="M5.5 11h13v4h-13z"/><path d="M7 15v6M17 15v6"/>"#;

/// A washing machine.
const WASHING_MACHINE: &str = r#"<rect x="4" y="2.5" width="16" height="19" rx="2"/><circle cx="12" cy="13.5" r="4.5"/><path d="M7.5 6h2"/><circle cx="16" cy="6" r=".6"/>"#;

/// A sprout, two leaves.
const SPROUT: &str = r#"<path d="M12 21v-9"/><path d="M12 12c0-3.5-2.5-6-6.5-6 0 3.5 2.5 6 6.5 6z"/><path d="M12 14c0-3.5 2.5-6 6.5-6 0 3.5-2.5 6-6.5 6z"/><path d="M7.5 21h9"/>"#;

/// A table lamp.
const LAMP: &str = r#"<path d="M8.5 3.5h7l2.5 7H6z"/><path d="M12 10.5v8.5"/><path d="M8.5 20.5h7"/>"#;

/// A fork and a knife.
const FORK_AND_KNIFE: &str = r#"<path d="M5.5 3v6a2.5 2.5 0 0 0 5 0V3M8 3v18"/><path d="M17.5 21V3c-2 1-3.5 4-3.5 8h3.5"/>"#;

/// A medical cross.
const MEDICAL_CROSS: &str = r#"<path d="M9.5 3.5h5v6h6v5h-6v6h-5v-6h-6v-5h6z"/>"#;

/// A bus from the front.
const BUS: &str = r#"<rect x="4" y="3.5" width="16" height="14" rx="2.5"/><path d="M4 11h16"/><path d="M7 17.5V20M17 17.5V20"/><circle cx="8" cy="14.3" r=".7"/><circle cx="16" cy="14.3" r=".7"/>"#;

/// An open book.
const OPEN_BOOK: &str = r#"<path d="M12 6.5c-2-1.5-5-2-8.5-1.5V19c3.5-.5 6.5 0 8.5 1.5 2-1.5 5-2 8.5-1.5V5c-3.5-.5-6.5 0-8.5 1.5z"/><path d="M12 6.5v14"/>"#;

/// A jacket, open at the collar.
const JACKET: &str = r#"<path d="M8.5 3.5 4 6l-.5 14.5H9V12M15.5 3.5 20 6l.5 14.5H15V12"/><path d="M8.5 3.5 12 8l3.5-4.5"/><path d="M12 8v12.5"/>"#;

/// A trainer.
const TRAINER: &str = r#"<path d="M3 18.5h18"/><path d="M3 18.5v-8a1 1 0 0 1 1-1h2.5a1 1 0 0 1 1 .8c.4 1.5 1.6 2.2 3 2.2h1l6.5 2a3 3 0 0 1 2 2.8v1.2"/><path d="m11.5 12.5-1.5-2.5M14.5 13.4l-1.3-2.2"/>"#;

/// A dress.
const DRESS: &str = r#"<path d="M9 3h6l-1 4.5 4.5 13.5h-13L10 7.5z"/><path d="M10 7.5h4"/>"#;

/// A handbag.
const HANDBAG: &str = r#"<path d="M5 8.5h14l-1.2 11.5a1 1 0 0 1-1 .9H7.2a1 1 0 0 1-1-.9z"/><path d="M8.5 8.5V7a3.5 3.5 0 0 1 7 0v1.5"/>"#;

/// A wristwatch.
const WRISTWATCH: &str = r#"<circle cx="12" cy="12" r="5.5"/><path d="M12 9.5V12l1.5 1.5"/><path d="M9 7l.7-4h4.6l.7 4M9 17l.7 4h4.6l.7-4"/>"#;

/// Sunglasses.
const SUNGLASSES: &str = r#"<circle cx="6.5" cy="14" r="3.5"/><circle cx="17.5" cy="14" r="3.5"/><path d="M10 14c1.3-1 2.7-1 4 0"/><path d="M3 14 4.5 8M21 14l-1.5-6"/>"#;

/// A bicycle.
const BICYCLE: &str = r#"<circle cx="5.5" cy="16" r="3.5"/><circle cx="18.5" cy="16" r="3.5"/><path d="M5.5 16 9 10.5h5.5l4 5.5"/><path d="M14.5 10.5 12.8 6.5h2.7"/><path d="M9 10.5 7.8 8.5H5"/>"#;

/// A dumbbell.
const DUMBBELL: &str = r#"<path d="M6.5 6.5v11M17.5 6.5v11M3.5 9v6M20.5 9v6M6.5 12h11"/>"#;

/// Two notes.
const NOTES: &str = r#"<path d="M9 18V5.5l11-2V16"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="17.5" cy="16" r="2.5"/>"#;

/// A tent.
const TENT: &str = r#"<path d="M2.5 20h19"/><path d="M4 20 12 5l8 15"/><path d="M9 20l3-6 3 6"/>"#;

/// A snowflake.
const SNOWFLAKE: &str = r#"<path d="M12 2.5v19M3.8 7.25l16.4 9.5M3.8 16.75l16.4-9.5"/><path d="m9.5 4 2.5 2 2.5-2M9.5 20l2.5-2 2.5 2"/>"#;

/// A dog's head, its ears down.
const DOG: &str = r#"<path d="M8 4.5c-2.5 0-4.5 2-4.5 5 0 1.5 1 2 2 1.5L7 10v5.5a5 5 0 0 0 10 0V10l1.5 1c1 .5 2 0 2-1.5 0-3-2-5-4.5-5z"/><circle cx="10" cy="12" r=".6"/><circle cx="14" cy="12" r=".6"/><path d="M11 16h2l-1 1z"/>"#;

/// A cat's head, its ears up.
const CAT: &str = r#"<path d="M5 4l3.5 4h7L19 4v9a7 7 0 0 1-14 0z"/><circle cx="9.5" cy="12.5" r=".6"/><circle cx="14.5" cy="12.5" r=".6"/><path d="M11 15.5h2l-1 1z"/>"#;

/// A bird.
const BIRD: &str = r#"<path d="M3.5 13.5c3 0 5-1.5 6-4a5 5 0 0 1 9.5 2l2.5-1-1.5 3a7 7 0 0 1-7 6.5H8z"/><circle cx="15" cy="10" r=".6"/>"#;

/// A fish.
const FISH: &str = r#"<path d="M2.5 12c3-4.5 9-6 14-2l3.5-3v10l-3.5-3c-5 4-11 2.5-14-2z"/><circle cx="7.5" cy="11.3" r=".6"/>"#;

/// A bone.
const BONE: &str = r#"<path d="M17.4 4.6a2.4 2.4 0 0 0-4.2 2.1L6.7 13.2a2.4 2.4 0 1 0-2.1 4.2 2.4 2.4 0 1 0 4.2 2.1l6.5-6.5a2.4 2.4 0 1 0 2.1-4.2 2.4 2.4 0 1 0 0-4.2z"/>"#;

/// An excavator, its arm and bucket.
const EXCAVATOR: &str = r#"<path d="M2.5 19.5h11"/><rect x="2.5" y="12" width="9" height="5" rx="1"/><path d="M5 12V9h4l1 3"/><path d="M11.5 13.5l5-6 4 3.5-1 2.5"/>"#;

/// A tractor.
const TRACTOR: &str = r#"<circle cx="7" cy="16" r="4"/><circle cx="18" cy="17.5" r="2.5"/><path d="M11 16h4.5M3.5 13V7.5h6l1.5 5.5"/><path d="M15.5 13V9.5h3.5l1.5 5.5"/>"#;

/// A solar panel on its stand.
const SOLAR_PANEL: &str = r#"<path d="M4.5 4.5h15l2 10h-19z"/><path d="M3.5 9.5h17M9.5 4.5l-1 10M14.5 4.5l1 10"/><path d="M12 14.5v5M8 19.5h8"/>"#;

/// A cog.
const COG: &str = r#"<path d="M18.4 10.6L20.9 10.7L20.9 13.3L18.4 13.4L17.5 15.5L19.2 17.4L17.4 19.2L15.5 17.5L13.4 18.4L13.3 20.9L10.7 20.9L10.6 18.4L8.5 17.5L6.6 19.2L4.8 17.4L6.5 15.5L5.6 13.4L3.1 13.3L3.1 10.7L5.6 10.6L6.5 8.5L4.8 6.6L6.6 4.8L8.5 6.5L10.6 5.6L10.7 3.1L13.3 3.1L13.4 5.6L15.5 6.5L17.4 4.8L19.2 6.6L17.5 8.5Z"/><circle cx="12" cy="12" r="2.5"/>"#;

/// A rubber stamp.
const STAMP: &str = r#"<circle cx="12" cy="5.5" r="2.5"/><path d="M10.5 8v3.5M13.5 8v3.5"/><rect x="4.5" y="11.5" width="15" height="4.5" rx="1"/><path d="M5.5 20h13"/>"#;

/// A number plate, its band at the left.
const NUMBER_PLATE: &str = r#"<rect x="2.5" y="7" width="19" height="10" rx="2"/><path d="M6.5 7v10"/><path d="M10 12h7.5"/>"#;

/// A leaf.
const LEAF: &str = r#"<path d="M5 19C5 10.5 10 5 19.5 4.5 19 14 13.5 19 5 19z"/><path d="M5 19l8.5-8.5"/>"#;

/// A car's wheels and axles, seen from above.
const AXLES: &str = r#"<rect x="3.5" y="3" width="3.5" height="5.5" rx="1"/><rect x="17" y="3" width="3.5" height="5.5" rx="1"/><rect x="3.5" y="15.5" width="3.5" height="5.5" rx="1"/><rect x="17" y="15.5" width="3.5" height="5.5" rx="1"/><path d="M7 5.75h10M7 18.25h10M12 5.75v12.5"/>"#;

/// A car's door, its window.
const CAR_DOOR: &str = r#"<path d="M4.5 20.5V12l7-7.5h8v16z"/><path d="M8 11.5l4.5-4.5H17v4.5z"/><path d="M14 15h3"/>"#;

/// A car seat.
const SEAT: &str = r#"<path d="M6.5 3.5h3l2 9h6a1.5 1.5 0 0 1 1.5 1.5v2.5H9z"/><path d="M10 16.5v4M17 16.5v4"/>"#;

/// Two people.
const PEOPLE: &str = r#"<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20.5a6.5 6.5 0 0 1 13 0"/><path d="M15.5 4.8a3.5 3.5 0 0 1 0 6.4M18 14.2a6.5 6.5 0 0 1 3.5 6.3"/>"#;

/// A shield with a check.
const SHIELD: &str = r#"<path d="M12 21.5c-4.5-2-7.5-5.5-7.5-10V5.5L12 2.5l7.5 3v6c0 4.5-3 8-7.5 10z"/><path d="m9 12 2 2 4-4"/>"#;

/// A closed book.
const BOOK: &str = r#"<path d="M5 18V4.5a2 2 0 0 1 2-2h12v14H7a2 2 0 0 0 0 4h12"/><path d="M9 7h6"/>"#;

/// A clipboard with a check.
const CLIPBOARD: &str = r#"<rect x="5" y="4" width="14" height="17.5" rx="2"/><rect x="8.5" y="2.5" width="7" height="3.5" rx="1"/><path d="m9 13.5 2 2 4-4"/>"#;

/// An umbrella.
const UMBRELLA: &str = r#"<path d="M3 12.5a9 9 0 0 1 18 0z"/><path d="M12 12.5v6a2 2 0 0 1-4 0"/><path d="M12 3.5v-1"/>"#;

/// A motorcyclist's helmet, its visor.
const HELMET: &str = r#"<path d="M3.5 18v-5a8.5 8.5 0 0 1 17 0v3.5a1.5 1.5 0 0 1-1.5 1.5z"/><path d="M20.5 10.5H13a2 2 0 0 0-2 2v.5a2 2 0 0 0 2 2h7.5"/>"#;

/// A licence card.
const ID_CARD: &str = r#"<rect x="2.5" y="5" width="19" height="14" rx="2"/><circle cx="8.5" cy="11" r="2"/><path d="M5.5 16a3 3 0 0 1 6 0"/><path d="M14.5 10h4M14.5 13.5h4"/>"#;

/// A van.
const VAN: &str = r#"<path d="M2.5 16.5V8a1.5 1.5 0 0 1 1.5-1.5h11l4.5 4.5 1.5.5a1.5 1.5 0 0 1 1 1.4v3.6h-2"/><path d="M8 16.5h7.5"/><circle cx="5.5" cy="16.5" r="2"/><circle cx="17.5" cy="16.5" r="2"/><path d="M15 6.5V11h4.5"/>"#;

/// A kettlebell.
const WEIGHT: &str = r#"<circle cx="12" cy="14.5" r="6.5"/><path d="M8.3 9.2 7.6 6.5A1.6 1.6 0 0 1 9.1 4.5h5.8a1.6 1.6 0 0 1 1.5 2l-.7 2.7"/>"#;

/// A number sign.
const HASH: &str = r#"<path d="M10 3.5 8 20.5M16 3.5l-2 17M4 9h16.5M3.5 15H20"/>"#;

/// A circle's width, across it.
const DIAMETER: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M7.5 12h9M9.5 10 7.5 12l2 2M14.5 10l2 2-2 2"/>"#;

/// A width between its arrows.
const WIDTH: &str = r#"<path d="M3.5 5v14M20.5 5v14"/><path d="M7 12h10M9.5 9.5 7 12l2.5 2.5M14.5 9.5 17 12l-2.5 2.5"/>"#;

/// A height between its arrows.
const HEIGHT: &str = r#"<path d="M5 3.5h14M5 20.5h14"/><path d="M12 7v10M9.5 9.5 12 7l2.5 2.5M9.5 14.5 12 17l2.5-2.5"/>"#;

/// A thermometer.
const THERMOMETER: &str = r#"<path d="M14 14.5V5a2 2 0 0 0-4 0v9.5a4 4 0 1 0 4 0z"/><path d="M12 11v6"/>"#;

/// Layers.
const LAYERS: &str = r#"<path d="m12 3 9 4.5-9 4.5-9-4.5z"/><path d="m3 12 9 4.5 9-4.5"/><path d="m3 16.5 9 4.5 9-4.5"/>"#;

/// A plug.
const PLUG: &str = r#"<path d="M9 2.5v4M15 2.5v4"/><path d="M6.5 6.5h11v4a5.5 5.5 0 0 1-11 0z"/><path d="M12 16v5.5"/>"#;

/// A floor's size, arrows across it.
const AREA: &str = r#"<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><path d="m8 16 8-8M11.5 8H16v4.5M8 11.5V16h4.5"/>"#;

/// A bathtub.
const BATH: &str = r#"<path d="M3 12h18v2.5a5 5 0 0 1-5 5H8a5 5 0 0 1-5-5z"/><path d="M5.5 12V6a2 2 0 0 1 3.9-.6"/><path d="M7.5 19.5 6.5 21.5M16.5 19.5l1 2"/>"#;

/// Stairs.
const STAIRS: &str = r#"<path d="M3.5 20.5h4.25v-4.25H12V12h4.25V7.75h4.25V3.5"/>"#;

/// A block of flats, floor over floor.
const BUILDING: &str = r#"<path d="M5.5 21V5l6.5-2.5L18.5 5v16"/><path d="M3.5 21h17"/><path d="M5.5 9h13M5.5 13h13M5.5 17h13"/>"#;

/// A sparkle.
const SPARKLES: &str = r#"<path d="M10 3 11.9 8.1 17 10 11.9 11.9 10 17 8.1 11.9 3 10 8.1 8.1z"/><path d="M18 14.5v5M15.5 17h5"/>"#;

/// A radiator.
const RADIATOR: &str = r#"<rect x="3.5" y="6" width="17" height="12" rx="2"/><path d="M8 6v12M12 6v12M16 6v12"/><path d="M6 18v2.5M18 18v2.5"/>"#;

/// A fence.
const FENCE: &str = r#"<path d="M5 20.5V7l2-2.5L9 7v13.5M15 20.5V7l2-2.5L19 7v13.5"/><path d="M3 10.5h18M3 16.5h18"/>"#;

/// Hills under the sun.
const HILLS: &str = r#"<path d="M2.5 19.5 8.5 11l4.5 6 3-4 5.5 6.5z"/><circle cx="16.5" cy="6.5" r="2"/>"#;

/// A document with a check.
const PERMIT: &str = r#"<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5v5h5"/><path d="m9 14.5 2 2 4-4"/>"#;

/// A road.
const ROAD: &str = r#"<path d="M7.5 3.5 4 20.5M16.5 3.5l3.5 17"/><path d="M12 4v3M12 10.5v3M12 17v3"/>"#;

/// A parking sign.
const PARKING: &str = r#"<rect x="3.5" y="3.5" width="17" height="17" rx="2.5"/><path d="M9.5 17V7.5h3.5a3 3 0 0 1 0 6H9.5"/>"#;

/// A banknote.
const BANKNOTE: &str = r#"<rect x="2.5" y="6" width="19" height="12" rx="2"/><circle cx="12" cy="12" r="2.5"/><path d="M6 12h.01M18 12h.01"/>"#;

/// A light bulb.
const LIGHT_BULB: &str = r#"<path d="M9 18h6M10 21h4"/><path d="M12 3a6 6 0 0 0-3.6 10.8c.6.5 1.1 1.3 1.1 2.1V16h5v-.1c0-.8.5-1.6 1.1-2.1A6 6 0 0 0 12 3z"/>"#;

/// A graduate's cap.
const MORTARBOARD: &str = r#"<path d="M2.5 9 12 4.5 21.5 9 12 13.5z"/><path d="M6.5 11v4.5c0 1.5 2.5 3 5.5 3s5.5-1.5 5.5-3V11"/><path d="M21.5 9v5"/>"#;

/// A memory card.
const MEMORY_CARD: &str = r#"<path d="M7 2.5h7.5l4 4V20a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 20V4A1.5 1.5 0 0 1 7 2.5z"/><path d="M9 6v3M12 6v3M15 7.5V9"/>"#;

/// A memory module.
const RAM: &str = r#"<path d="M2.5 7h19v9h-8.5l-1 1.5-1-1.5H2.5z"/><path d="M6 10.5v2M9.5 10.5v2M14.5 10.5v2M18 10.5v2"/><path d="M5 16v2.5M19 16v2.5"/>"#;

/// A battery.
const BATTERY: &str = r#"<rect x="2.5" y="7" width="16" height="10" rx="2"/><path d="M21.5 10.5v3"/><path d="M6 10.5v3M9.5 10.5v3"/>"#;

/// A chip.
const CHIP: &str = r#"<rect x="6" y="6" width="12" height="12" rx="1.5"/><rect x="9.5" y="9.5" width="5" height="5" rx=".5"/><path d="M9.5 2.5V6M14.5 2.5V6M9.5 18v3.5M14.5 18v3.5M2.5 9.5H6M2.5 14.5H6M18 9.5h3.5M18 14.5h3.5"/>"#;

/// A drive.
const DRIVE: &str = r#"<rect x="2.5" y="7" width="19" height="10" rx="2"/><path d="M6 12h.01M9.5 12h.01M13.5 12h4"/>"#;

/// A graphics card, its fans.
const GRAPHICS_CARD: &str = r#"<rect x="2.5" y="6" width="19" height="10.5" rx="1.5"/><circle cx="8.5" cy="11.25" r="2.75"/><circle cx="15.5" cy="11.25" r="2.75"/><path d="M6 16.5V19M10 16.5V19M14 16.5V19"/>"#;

/// Four windows.
const APPS: &str = r#"<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>"#;

/// A screen's diagonal.
const DIAGONAL: &str = r#"<rect x="2.5" y="4.5" width="19" height="13" rx="2"/><path d="m7.5 13.5 9-5M13 8.5h3.5v3M11 13.5H7.5v-3"/><path d="M8 21h8"/>"#;

/// Wi-Fi.
const WIFI: &str = r#"<path d="M2.5 9.5a13.5 13.5 0 0 1 19 0M5.5 13a9 9 0 0 1 13 0M8.8 16.4a4.5 4.5 0 0 1 6.4 0"/><path d="M12 19.5h.01"/>"#;

/// A screen's pixels.
const PIXELS: &str = r#"<rect x="2.5" y="4.5" width="19" height="13" rx="2"/><path d="M2.5 11h19M9 4.5v13M15 4.5v13"/><path d="M8 21h8"/>"#;

/// A joystick.
const JOYSTICK: &str = r#"<path d="M12 12V7.5"/><circle cx="12" cy="5" r="2.5"/><rect x="4.5" y="12" width="15" height="7.5" rx="2"/><path d="M8 15.75h2"/>"#;

/// A disc.
const DISC: &str = r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="2.5"/><path d="M6.5 12A5.5 5.5 0 0 1 12 6.5"/>"#;

/// A sensor in its frame.
const SENSOR: &str = r#"<rect x="3.5" y="5.5" width="17" height="13" rx="2"/><rect x="7.5" y="9" width="9" height="6" rx="1"/>"#;

/// A lens and its mount.
const LENS: &str = r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/><path d="M12 3.5v2M20.5 12h-2M12 20.5v-2M3.5 12h2"/>"#;

/// A shutter's blades.
const SHUTTER: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5 14.5 12M20.5 12 12 14.5M12 20.5 9.5 12M3.5 12 12 9.5"/>"#;

/// Bluetooth.
const BLUETOOTH: &str = r#"<path d="m7 7 10 10-5 4.5v-19L17 7 7 17"/>"#;

/// A loudspeaker, silenced.
const MUTE: &str = r#"<path d="M3.5 9.5h3.5L12 5v14l-5-4.5H3.5z"/><path d="m16 9.5 5 5M21 9.5l-5 5"/>"#;

/// An open padlock.
const OPEN_LOCK: &str = r#"<rect x="4.5" y="11" width="15" height="10.5" rx="2"/><path d="M8 11V7.5a4 4 0 0 1 7.7-1.5"/>"#;

/// A swatch of material.
const TEXTURE: &str = r#"<rect x="3.5" y="3.5" width="17" height="17" rx="2.5"/><path d="M3.5 12 12 3.5M3.5 19 19 3.5M10 20.5 20.5 10"/>"#;

/// A ruler.
const RULER: &str = r#"<path d="M3 16.5 16.5 3 21 7.5 7.5 21z"/><path d="m7.5 12 2 2M10.5 9l2 2M13.5 6l2 2"/>"#;

/// The registered trade mark.
const BRAND: &str = r#"<circle cx="12" cy="12" r="9"/><path d="M9.5 16.5v-9h3.5a2.5 2.5 0 0 1 0 5H9.5M13 12.5l2.5 4"/>"#;

/// An energy label's arrows.
const ENERGY: &str = r#"<path d="M3.5 6h6M3.5 12h10M3.5 18h14"/><path d="m9.5 4.5 1.5 1.5-1.5 1.5M13.5 10.5l1.5 1.5-1.5 1.5M17.5 16.5l1.5 1.5-1.5 1.5"/>"#;

/// A measuring jug.
const JUG: &str = r#"<path d="M5.5 4.5h11l-1 15.5a1.5 1.5 0 0 1-1.5 1.5H8a1.5 1.5 0 0 1-1.5-1.5z"/><path d="M16.3 7.5h1.7a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2h-2.2"/><path d="M9 9.5h3M9 13h2M9 16.5h3"/>"#;

/// A clock: reserving a listing, and a listing about to expire (notifications.rs).
pub const CLOCK: &str = r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#;

/// A document.
const DOCUMENT: &str = r#"<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5v5h5"/><path d="M8.5 12.5h7M8.5 16h5"/>"#;

/// A star.
const STAR: &str = r#"<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3l-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z"/>"#;

/// Two arrows, each round to the other.
const SHIFTS: &str = r#"<path d="m17 2.5 3.5 3.5-3.5 3.5"/><path d="M3.5 11.5V10a4 4 0 0 1 4-4h13"/><path d="m7 21.5-3.5-3.5 3.5-3.5"/><path d="M20.5 12.5V14a4 4 0 0 1-4 4h-13"/>"#;

/// A folded map.
const MAP: &str = r#"<path d="M3.5 6.5 9 4l6 2.5L20.5 4v13.5L15 20l-6-2.5-5.5 2.5z"/><path d="M9 4v13.5M15 6.5V20"/>"#;

/// A receipt.
const RECEIPT: &str = r#"<path d="M5.5 2.5h13v19l-2.2-1.5-2.1 1.5-2.2-1.5-2.2 1.5-2.1-1.5-2.2 1.5z"/><path d="M9 7.5h6M9 11h6M9 14.5h4"/>"#;

/// A person.
const PERSON: &str = r#"<circle cx="12" cy="7.5" r="4"/><path d="M4.5 21a7.5 7.5 0 0 1 15 0"/>"#;

/// A tape measure.
const TAPE_MEASURE: &str = r#"<circle cx="10" cy="10" r="7"/><circle cx="10" cy="10" r="2"/><path d="M10 17h11.5v-3"/><path d="M14.5 17v-1.5M18 17v-1.5"/>"#;

/// Two footprints.
const FOOTPRINTS: &str = r#"<path d="M5.5 15.5c-1-.5-1.8-2.4-1.8-4.7 0-3 1.3-5.3 3-5.3s2.8 2 2.8 4.6c0 1.8-.6 3.7-1.2 5.4z"/><path d="M4.8 18.2a1.6 1.6 0 0 0 3.1.6l.3-1.3-3-.8z"/><path d="M18.5 11.5c1-.5 1.8-2.4 1.8-4.7 0-3-1.3-5.3-3-5.3s-2.8 2-2.8 4.6c0 1.8.6 3.7 1.2 5.4z"/><path d="M19.2 14.2a1.6 1.6 0 0 1-3.1.6l-.3-1.3 3-.8z"/>"#;

/// A spoked wheel.
const WHEEL: &str = r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="1.5"/><path d="M12 3.5v7M12 13.5v7M3.5 12h7M13.5 12h7"/>"#;

/// A sound wave.
const SOUND_WAVE: &str = r#"<path d="M2.5 12h2.5l2-4 3 8 2.5-11 2.5 11 3-8 2 4h1.5"/>"#;

/// A cake and its candle.
const CAKE: &str = r#"<path d="M3.5 21h17"/><path d="M5 21v-8a1.5 1.5 0 0 1 1.5-1.5h11A1.5 1.5 0 0 1 19 13v8"/><path d="M5 16c1.2 0 1.8-1 3.5-1s2.3 1 3.5 1 1.8-1 3.5-1 2.3 1 3.5 1"/><path d="M12 11.5v-3"/><path d="M12 5.5h.01"/>"#;

/// The signs of the sexes, together.
const SEXES: &str = r#"<circle cx="10" cy="10" r="4.5"/><path d="M10 14.5v7M7 18.5h6"/><path d="M13.2 6.8 18 2M14.5 2H18v3.5"/>"#;

/// A syringe.
const SYRINGE: &str = r#"<rect x="10" y="4.5" width="4.5" height="11" rx="1" transform="rotate(45 12.25 10)"/><path d="M16.1 6.1 19.5 2.7M17.8 1l3.2 3.2"/><path d="M8.4 13.9 3.5 18.8"/>"#;

/// A rosette.
const ROSETTE: &str = r#"<circle cx="12" cy="9" r="5.5"/><path d="M8.5 13.2 7 21.5l5-2.5 5 2.5-1.5-8.3"/>"#;

/// A stopwatch.
const STOPWATCH: &str = r#"<circle cx="12" cy="13.5" r="7.5"/><path d="M12 13.5V10M9.5 2.5h5M12 2.5V6"/><path d="M18 7.5l1.5-1.5"/>"#;

/// A sine wave.
const SINE_WAVE: &str = r#"<path d="M2.5 12c1.6-4.7 4.7-4.7 6.3 0s4.7 4.7 6.4 0 4.7-4.7 6.3 0"/>"#;

/// An engine.
const ENGINE: &str = r#"<path d="M12 4.5v3M9.5 4.5h5"/><path d="M6.5 10h2l2-2.5h4.2l1.8 3h1.5V9h2v8h-2v-2h-2.5v2.5H10L8 15H6.5z"/><path d="M3.5 10v5M3.5 12.5h3"/>"#;

/// A computer's tower.
const TOWER: &str = r#"<rect x="7" y="2.5" width="10" height="19" rx="1.5"/><path d="M10 6.5h4M10 9.5h4"/><circle cx="12" cy="16.5" r="1"/>"#;

/// A door.
const DOOR: &str = r#"<path d="M5.5 21V4a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1v17"/><path d="M3.5 21h17"/><path d="M14.5 12h.01"/>"#;

// ---------------------------------------------------------------- categories

/// An eye: how many times a listing was seen (listing_card.html, listing.html, my_listings.html).
pub const EYE: &str = r#"<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="3"/>"#;

/// A heart: the listings a visitor keeps (the header's button in base.html, the welcome page's at
/// the end of its categories in home.html, heart.html over a listing's photo), and how many keep a
/// seller's listing (my_listings.html).
pub const HEART: &str = r#"<path d="M12 20.3 4.6 13.1A4.9 4.9 0 0 1 11.5 6.2l.5.5.5-.5a4.9 4.9 0 0 1 6.9 6.9Z"/>"#;

/// A flag: reporting a listing (listing.html).
pub const FLAG: &str = r#"<path d="M5.5 21V4"/><path d="M5.5 4h12l-2.5 4 2.5 4h-12"/>"#;

/// A circle crossed out: blocking a seller (listing.html).
pub const BLOCK: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="m6 6 12 12"/>"#;
/// The seller's own buttons on their listing's page (listing.html): a pencil for editing it
/// ("Ndrysho"), two bars for taking it down for a while ("Çaktivizo"), a bin for deleting it
/// ("Fshi").
pub const PENCIL: &str = r#"<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>"#;
pub const PAUSE: &str = r#"<rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/>"#;
pub const TRASH: &str = r#"<path d="M3 6h18"/><path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M10 11v6M14 11v6"/>"#;
/// And a circle with a check for saying a listing is sold or let out ("U shit", "U dha me qira"),
/// the clock for reserving it ("Rezervo") or taking the reservation off ("Hiq rezervimin"), and
/// a triangle for putting a paused or sold one back up ("Aktivizo").
pub const SOLD: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="m8.5 12 2.5 2.5 4.5-5"/>"#;
pub const RESERVE: &str = CLOCK;
pub const RESUME: &str = r#"<path d="M8 5.5v13l10.5-6.5Z"/>"#;
/// Two arrows round a circle: renewing a listing that expired or soon will ("Rifresko").
pub const RENEW: &str = r#"<path d="M20 11a8 8 0 0 0-14.3-4.9L4 8"/><path d="M4 4v4h4"/><path d="M4 13a8 8 0 0 0 14.3 4.9L20 16"/><path d="M20 20v-4h-4"/>"#;

/// A shield with a check: a seller's number or address verified (listing.html, profile.html).
pub const VERIFIED: &str = r#"<path d="M12 3 4.5 6v5.5c0 4.5 3.2 8.3 7.5 9.5 4.3-1.2 7.5-5 7.5-9.5V6z"/><path d="m8.5 12 2.5 2.5 4.5-5"/>"#;

/// A telephone's handset, for a call: the button that shows a seller's number (phone.svg), and how
/// many times it was shown (my_listings.html).
pub const CALL: &str = r#"<path d="M5 4h3.2l1.6 4.2-2.1 1.4a11 11 0 0 0 6.7 6.7l1.4-2.1 4.2 1.6V19a2 2 0 0 1-2.2 2A17 17 0 0 1 3 6.2 2 2 0 0 1 5 4Z"/>"#;

/// A speech bubble: messages (the header's button in base.html, the welcome page's in the row with
/// its search in home.html, and the button on a listing's page that writes to its seller).
pub const CHAT: &str = r#"<path d="M20.5 11.5a8 8 0 0 1-11.6 7.2L4 20l1.3-4.4A8 8 0 1 1 20.5 11.5Z"/>"#;

/// WhatsApp's mark as a line drawing: a round bubble, its tail at the lower left, and a handset
/// filled in it; the button beside a seller's revealed number that opens a chat with it
/// (phone_stream.html).
pub const WHATSAPP: &str = r#"<path d="M4.6 16.4A8.6 8.6 0 1 1 7.6 19.4L3.2 20.8Z"/><path fill="currentColor" stroke="none" d="M9.2 8.1c.4-.4 1-.3 1.3.1l.9 1.5c.2.4.1.8-.2 1.1l-.5.5c.5 1.1 1.4 2 2.5 2.5l.5-.5c.3-.3.7-.4 1.1-.2l1.5.9c.4.3.5.9.1 1.3l-.6.6c-.7.7-1.7.8-2.6.4a9 9 0 0 1-4.4-4.4c-.4-.9-.3-1.9.4-2.6z"/>"#;

/// A crescent moon: the theme's button on the light theme, which switches to the dark one (the
/// header's in base.html, the welcome page's at the end of its categories in home.html).
pub const MOON: &str = r#"<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z"/>"#;

/// A sun: the theme's button on the dark theme, which switches back to the light one.
pub const SUN: &str = r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#;

/// A bell: keeping a search to hear of its new listings ("Ruaj kërkimin", browse.html), and the
/// searches kept (saved.html, saved_searches.html).
pub const BELL: &str = r#"<path d="M6.5 10a5.5 5.5 0 0 1 11 0c0 4.8 2 6.8 2 6.8h-15s2-2 2-6.8Z"/><path d="M10 20a2.2 2.2 0 0 0 4 0"/>"#;

/// A megaphone, its mouth to the right, and the handle under it: a listing promoted
/// (notifications.rs).
pub const MEGAPHONE: &str = r#"<path d="M3.5 10.5v3a1 1 0 0 0 1 1h2l10 4.5v-14l-10 4.5h-2a1 1 0 0 0-1 1Z"/><path d="M7.5 14.5 9 19.5h2.5"/><path d="M20 10v4"/>"#;

/// A storefront: its awning, scalloped, over its walls and door; an invitation into a store
/// (notifications.rs).
pub const STORE: &str = r#"<path d="M4.5 4h15L21 9H3z"/><path d="M3 9a3 3 0 0 0 6 0 3 3 0 0 0 6 0 3 3 0 0 0 6 0"/><path d="M4.5 11.5V20h15v-8.5"/><path d="M10 20v-4.5h4V20"/>"#;

/// A paper plane: sending a message (conversation.html).
pub const SEND: &str = r#"<path d="m21 3-6.5 18-4-7.5-7.5-4Z"/><path d="M21 3 10.5 13.5"/>"#;

// ---------------------------------------------------------------- the redesign's

// Drawn as the "Calmer Pluribus" canvas has them, before the pages that take them are redrawn:
// each is expected to be unused until then, and the compiler says when that stops being so (an
// unfulfilled expectation), for its mark to come off as its page takes it.

/// Three lines: the menu (the header's "Menyja" button, which opens the drawer).
#[expect(dead_code, reason = "the header's menu button takes it when it is redrawn")]
pub const MENU: &str = r#"<path d="M4 6.5h16M4 12h16M4 17.5h16"/>"#;

/// A bookmark: keeping a search ("Ruaj kërkimin", its mark in the accent), and the searches kept
/// (the menu's "Kërkimet e ruajtura", saved_searches.html), where the bell is now; the bell stays
/// for the notifications.
#[expect(dead_code, reason = "Ruaj kërkimin and the menu take it when they are redrawn")]
pub const BOOKMARK: &str = r#"<path d="M6.5 3.5h11v17l-5.5-4-5.5 4z"/>"#;

/// A globe: the language (the menu's "Gjuha", Shqip | English).
#[expect(dead_code, reason = "the menu takes it when it is drawn")]
pub const GLOBE: &str = r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a13.5 13.5 0 0 1 0 18 13.5 13.5 0 0 1 0-18Z"/>"#;

/// A big A and a small one: the text's size (the menu's "Madhësia e shkrimit").
#[expect(dead_code, reason = "the menu takes it when it is drawn")]
pub const TEXT_SIZE: &str = r#"<path d="M4 19 9 5l5 14M5.8 14h6.4"/><path d="M15 19l3-8 3 8M15.9 16.5h4.2"/>"#;

/// A door's frame and an arrow out of it: signing out ("Dil nga llogaria", in "Njoftimet e
/// mia" and "Cilësimet").
#[expect(dead_code, reason = "the account's pages take it when they are redrawn")]
pub const LOG_OUT: &str = r#"<path d="M14 4.5h3.5a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H14"/><path d="M10 16.5 5.5 12 10 7.5M5.5 12h10"/>"#;

/// An i in a circle: a note on how things stand (a seller's own listing awaiting review,
/// expired or paused), and the page that something went wrong on.
#[expect(dead_code, reason = "the notices and the error page take it when they are redrawn")]
pub const INFO: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>"#;

/// A crosshair: finding where the visitor is (the map's "Vendndodhja ime", where the pin is now).
#[expect(dead_code, reason = "the map's panel takes it when it is redrawn")]
pub const LOCATE: &str = r#"<circle cx="12" cy="12" r="7"/><circle cx="12" cy="12" r="2.5"/><path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5"/>"#;

/// An office block with a lower wing: a store without a logo of its own, in its round avatar (a
/// listing's seller row, its page, an invitation into it: seller_card.html, profile.html,
/// store_invitation.html), as a person has the initial of their name.
pub const BUSINESS: &str = r#"<rect x="4" y="3.5" width="11" height="17" rx="1.5"/><path d="M15 9.5h4a1 1 0 0 1 1 1v10H4"/><path d="M7.5 7.5h1M10.5 7.5h1M7.5 11h1M10.5 11h1M7.5 14.5h1M10.5 14.5h1"/>"#;

/// Other things, and a category without an icon of its own yet: a box.
const BOX: &str = r#"<path d="M3.5 7.5 12 3l8.5 4.5v9L12 21l-8.5-4.5z"/><path d="M3.5 7.5 12 12l8.5-4.5"/><path d="M12 12v9"/>"#;

/// A car, from the side.
const CAR: &str = r#"<path d="M5.5 11 7 7a2 2 0 0 1 1.9-1.4h6.2A2 2 0 0 1 17 7l1.5 4"/><path d="M5.2 16.5H4a1 1 0 0 1-1-1V13a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v2.5a1 1 0 0 1-1 1h-1.2"/><path d="M9.8 16.5h4.4"/><circle cx="7.5" cy="16.5" r="2.2"/><circle cx="16.5" cy="16.5" r="2.2"/>"#;

/// A phone.
const HANDSET: &str = r#"<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M10.5 18h3"/>"#;

/// A briefcase.
const BRIEFCASE: &str = r#"<rect x="3" y="7" width="18" height="13" rx="2.5"/><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7"/><path d="M3 12.5h18"/>"#;

/// A hammer.
const HAMMER: &str = r#"<path d="m4.5 19.5 9-9"/><rect x="11" y="3.8" width="10" height="5.2" rx="1.3" transform="rotate(45 16 6.4)"/>"#;

/// A T-shirt.
const T_SHIRT: &str = r#"<path d="M8.5 3.5 3.5 6l-1.5 4.5 3.5 1.5v8.5h13V12l3.5-1.5-1.5-4.5-5-2.5a3.5 3.5 0 0 1-7 0z"/>"#;

/// A tennis ball.
const TENNIS_BALL: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M5.8 6.2a8.5 8.5 0 0 1 0 11.6M18.2 6.2a8.5 8.5 0 0 0 0 11.6"/>"#;

/// A factory.
const FACTORY: &str = r#"<path d="M3 20.5V10l5 3v-3l5 3v-3l5 3V4.5h3v16z"/><path d="M7 17h2M11.5 17h2M16 17h2"/>"#;

/// A wrench.
const WRENCH: &str = r#"<path d="M15.2 4.3a4.6 4.6 0 0 0-5.4 6l-6.2 6.2a2 2 0 1 0 2.9 2.9l6.2-6.2a4.6 4.6 0 0 0 6-5.4l-2.9 2.9-2.6-.4-.4-2.6z"/>"#;

/// A nut.
const NUT: &str = r#"<path d="M12 3 19.8 7.5v9L12 21l-7.8-4.5v-9z"/><circle cx="12" cy="12" r="3.5"/>"#;

/// A loudspeaker.
const LOUDSPEAKER: &str = r#"<rect x="5.5" y="2.5" width="13" height="19" rx="2.5"/><circle cx="12" cy="14.5" r="3.5"/><circle cx="12" cy="7" r="1.2"/>"#;

/// A navigation arrow.
const NAVIGATION_ARROW: &str = r#"<path d="M12 3.5 19 20l-7-3.8L5 20z"/>"#;

/// A racing flag.
const RACING_FLAG: &str = r#"<path d="M5 21V3.5"/><path d="M5 4h12.5l-2.5 4.5 2.5 4.5H5"/>"#;

/// A mat with its grooves.
const MAT: &str = r#"<rect x="5.5" y="3" width="13" height="18" rx="3"/><path d="M9 7v10M12 7v10M15 7v10"/>"#;

/// A trailer: its box, wheel and drawbar.
const TRAILER: &str = r#"<path d="M2.5 15.5v-7a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1v7z"/><path d="M15.5 13.5h6"/><circle cx="9" cy="17.5" r="2"/>"#;

/// Two blocks of flats.
const FLATS: &str = r#"<path d="M3.5 21V8.5l6-3V21"/><path d="M9.5 21V3.5h11V21"/><path d="M2.5 21h19"/><path d="M13 7.5h1M17 7.5h1M13 11.5h1M17 11.5h1M13 15.5h1M17 15.5h1M6 12h1M6 16h1"/>"#;

/// A house with a chimney and a door.
const VILLA: &str = r#"<path d="M3 11 12 4l9 7"/><path d="M5 9.5V20h14V9.5"/><path d="M15.5 6.5V4h2v4"/><rect x="9.5" y="14" width="5" height="6"/>"#;

/// A shop front under its awning.
const SHOP: &str = r#"<path d="M3.5 9.5 5 4h14l1.5 5.5"/><path d="M3.5 9.5a2.8 2.8 0 0 0 5.7 0 2.8 2.8 0 0 0 5.6 0 2.8 2.8 0 0 0 5.7 0"/><path d="M5 12v8.5h14V12"/><path d="M10 20.5v-5h4v5"/>"#;

/// A plot of land, a flag in it.
const PLOT: &str = r#"<path d="M2.5 17.5 7 11h10l4.5 6.5z"/><path d="M12 11V4.5"/><path d="M12 4.5h4.5l-1.2 1.75 1.2 1.75H12"/>"#;

/// A garage and its door.
const GARAGE: &str = r#"<path d="M3 20.5V9l9-5.5L21 9v11.5"/><path d="M7 20.5v-8h10v8"/><path d="M7 15h10M7 17.8h10"/>"#;

/// A hard hat.
const HARD_HAT: &str = r#"<path d="M2.5 18h19"/><path d="M5 18v-2.5a7 7 0 0 1 14 0V18"/><path d="M9.5 9.3V6.5h5v2.8"/><path d="M12 6.5V13"/>"#;

/// Code brackets.
const CODE: &str = r#"<path d="m8 7-5 5 5 5M16 7l5 5-5 5M14 4.5l-4 15"/>"#;

/// A price tag: also a kept listing's price dropped (notifications.rs).
pub const PRICE_TAG: &str = r#"<path d="M3.5 12.5V4.5a1 1 0 0 1 1-1h8l8.5 8.5a1.5 1.5 0 0 1 0 2.1l-6.9 6.9a1.5 1.5 0 0 1-2.1 0z"/><circle cx="8" cy="8" r="1.4"/>"#;

/// A steering wheel.
const STEERING_WHEEL: &str = r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="2"/><path d="M3.8 10.5c3.5-1 12.9-1 16.4 0M10.2 13.5 7 19.5M13.8 13.5l3.2 6"/>"#;

/// A headset and its microphone.
const HEADSET: &str = r#"<path d="M4 13v-1a8 8 0 0 1 16 0v1"/><rect x="2.5" y="12.5" width="4" height="6" rx="1.5"/><rect x="17.5" y="12.5" width="4" height="6" rx="1.5"/><path d="M19.5 18.5a3 3 0 0 1-3 3H13"/>"#;

/// A screwdriver.
const SCREWDRIVER: &str = r#"<rect x="13.5" y="2.6" width="4.5" height="8.5" rx="1.5" transform="rotate(45 15.75 6.85)"/><path d="m12.6 11.4-8 8"/>"#;

/// A broom.
const BROOM: &str = r#"<path d="M14 3.5 10.5 12"/><path d="M6.5 12h8l2 8.5h-12z"/><path d="M9 16.5l-.5 4M12 16.5v4M15 16.5l.5 4"/>"#;

/// A video camera.
const VIDEO_CAMERA: &str = r#"<rect x="2.5" y="6.5" width="13" height="11" rx="2"/><path d="m15.5 10.5 6-3.5v10l-6-3.5"/>"#;

/// A paint roller.
const PAINT_ROLLER: &str = r#"<rect x="3.5" y="3.5" width="14" height="5" rx="1.5"/><path d="M17.5 6h2a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1h-7.5v3"/><rect x="10.5" y="14" width="3" height="7" rx="1"/>"#;

/// The welcome page's main categories, one icon each, for its shortcuts under the search
/// (browse.rs); a main category added later gets BOX until it has one of its own.
const MAIN_CATEGORIES: [(&str, &str); 11] = [
    ("automjete", CAR),
    ("prona", HOUSE),
    ("elektronike", HANDSET),
    ("shtepi-dhe-kopsht", SOFA),
    ("pune", BRIEFCASE),
    ("sherbime", HAMMER),
    ("veshje-dhe-aksesore", T_SHIRT),
    ("sport-dhe-hobi", TENNIS_BALL),
    ("kafshe-shtepiake", PAW),
    ("makineri-dhe-biznes", FACTORY),
    ("te-tjera", BOX),
];

/// Subcategories' icons, for their pills under a category page's heading (browse.rs): a subcategory
/// without one shows its name alone.
const SUBCATEGORIES: [(&str, &str); 66] = [
    ("makina", CAR_FRONT),
    ("motocikleta", MOTORBIKE),
    ("automjete-tregtare", TRUCK),
    ("pjese-dhe-aksesore", WRENCH),
    ("pjese-kembimi", NUT),
    ("goma-dhe-disqe", TYRE),
    ("audio-dhe-video", LOUDSPEAKER),
    ("gps-dhe-navigatore", NAVIGATION_ARROW),
    ("styling-dhe-pajisje-sportive", RACING_FLAG),
    ("tapete-makine", MAT),
    ("rimorkio", TRAILER),
    ("mbushes-per-makina-elektrike", BOLT),
    ("pjese-te-tjera", BOX),
    ("apartamente", FLATS),
    ("garsoniera", BED),
    ("vila-dhe-shtepi", VILLA),
    ("dyqane", SHOP),
    ("zyra", OFFICE),
    ("truall", PLOT),
    ("garazhe", GARAGE),
    ("telefona", PHONE),
    ("laptope", LAPTOP),
    ("kompjutere", SCREEN),
    ("tableta", TABLET),
    ("televizore", TV),
    ("konsola-lojerash", GAMEPAD),
    ("aparate-fotografike", CAMERA),
    ("audio-dhe-kufje", HEADPHONES),
    ("mobilje", CHAIR),
    ("elektroshtepiake", WASHING_MACHINE),
    ("kopsht", SPROUT),
    ("dekor", LAMP),
    ("pune-turizem", FORK_AND_KNIFE),
    ("pune-ndertim", HARD_HAT),
    ("pune-shendetesi", MEDICAL_CROSS),
    ("pune-it", CODE),
    ("pune-shitje", PRICE_TAG),
    ("pune-transport", STEERING_WHEEL),
    ("pune-call-center", HEADSET),
    ("riparime", SCREWDRIVER),
    ("transport-dhe-transferime", BUS),
    ("pastrim", BROOM),
    ("mesime", OPEN_BOOK),
    ("foto-dhe-video", VIDEO_CAMERA),
    ("ndertim-dhe-lyerje", PAINT_ROLLER),
    ("xhaketa-dhe-pallto", JACKET),
    ("kepuce", TRAINER),
    ("fustane", DRESS),
    ("canta", HANDBAG),
    ("ora", WRISTWATCH),
    ("aksesore", SUNGLASSES),
    ("bicikleta", BICYCLE),
    ("fitnes", DUMBBELL),
    ("instrumente-muzikore", NOTES),
    ("kamping-dhe-peshkim", TENT),
    ("sporte-dimerore", SNOWFLAKE),
    ("sport-te-tjera", BOX),
    ("qen", DOG),
    ("mace", CAT),
    ("zogj", BIRD),
    ("peshq", FISH),
    ("aksesore-per-kafshe", BONE),
    ("makineri-ndertimi", EXCAVATOR),
    ("makineri-bujqesore", TRACTOR),
    ("energji-dhe-gjeneratore", SOLAR_PANEL),
    ("makineri-industriale", COG),
];

/// A main category's icon, by its slug: BOX for one without its own.
pub fn main_category(slug: &str) -> &'static str {
    MAIN_CATEGORIES.iter().find(|(of, _)| *of == slug).map_or(BOX, |(_, icon)| *icon)
}

/// A subcategory's icon, by its slug, where it has one.
pub fn subcategory(slug: &str) -> Option<&'static str> {
    SUBCATEGORIES.iter().find(|(of, _)| *of == slug).map(|(_, icon)| *icon)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn a_property_takes_its_categorys_look_from_any_level_above() {
        let breed = |category: &str| look(&[category, "kafshe-shtepiake"], "pet_breed").map(|look| look.icon);
        assert_eq!(breed("qen"), Some(DOG));
        assert_eq!(breed("mace"), Some(CAT));
        assert_eq!(look(&["telefona", "elektronike"], "electronics_brand").map(|look| look.icon), Some(PHONE));
        assert_eq!(look(&["kompjutere", "elektronike"], "electronics_brand").map(|look| look.icon), Some(BRAND));
        // Two levels up, a kind of part's make is the vehicle's, named; a car's reads alone.
        let part = look(&["gps-dhe-navigatore", "pjese-dhe-aksesore", "automjete"], "vehicle_make").unwrap();
        assert_eq!((part.icon, part.reads), (CAR_FRONT, Named));
        assert_eq!(look(&["makina", "automjete"], "vehicle_make").map(|look| look.reads), Some(Alone));
        assert!(look(&["apartamente", "prona"], "property_features").is_none());
    }

    #[test]
    fn each_property_has_one_look_of_its_own() {
        for (at, (code, ..)) in PROPERTIES.iter().enumerate() {
            assert!(!PROPERTIES[..at].iter().any(|(other, ..)| other == code), "{code} twice");
        }
        for (category, code, ..) in IN_CATEGORIES {
            assert!(PROPERTIES.iter().any(|(of, ..)| *of == code), "{category}: {code} has no look of its own");
        }
    }

    /// Every property a listing's page shows, in every category that takes listings, has its
    /// icon there; and those whose values read alone have icons no other's has, so that no two
    /// of a listing's pills look alike without a name to tell them apart.
    #[sqlx::test]
    async fn every_property_a_listing_shows_has_an_icon(pool: PgPool) -> sqlx::Result<()> {
        let categories: Vec<(Uuid, Option<Uuid>, String)> =
            sqlx::query_as("SELECT id, parent_id, slug FROM categories").fetch_all(&pool).await?;
        let parents: HashMap<Uuid, (Option<Uuid>, &str)> =
            categories.iter().map(|(id, parent, slug)| (*id, (*parent, slug.as_str()))).collect();
        let shown: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT e.category_id, a.code
             FROM effective_category_attributes e
             JOIN attributes a ON a.id = e.attribute_id
             JOIN categories c ON c.id = e.category_id
             WHERE e.show_in_details AND a.data_type <> 'MULTISELECT' AND c.allow_listings",
        )
        .fetch_all(&pool)
        .await?;
        assert!(shown.len() > 300, "{}", shown.len());
        let mut alone: HashMap<(Uuid, &str), &str> = HashMap::new();
        for (category, code) in &shown {
            let mut slugs = Vec::new();
            let mut at = Some(*category);
            while let Some((parent, slug)) = at.and_then(|id| parents.get(&id)) {
                slugs.push(*slug);
                at = *parent;
            }
            let seen = look(&slugs, code).unwrap_or_else(|| panic!("{code} has no icon in {}", slugs[0]));
            if seen.reads == Alone
                && let Some(other) = alone.insert((*category, seen.icon), code.as_str())
            {
                panic!("{code} and {other} look alike in {}", slugs[0]);
            }
        }
        Ok(())
    }
}
