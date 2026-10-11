//! The site's icons, Phosphor's (phosphoricons.com, MIT: static/vendor/phosphor/LICENSE; the user's
//! ask of 2026-10-11), each the inside of an SVG whose box is 24 units: the icon's 256-unit drawing
//! scaled into it, its lines filled in currentColor and the tone shape of its Duotone weight under
//! them in the svg's own fill, none at rest, the accent where a hover or a press fills an icon
//! (base.css), the wrapper's stroke left aside. Among them the icons of a listing's details
//! (listing.html): one before each property's value, drawn as the welcome page's categories are
//! (browse.rs). Where the category says more than the property, the icon is the category's: a phone's
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
/// Phosphor's `car`.
pub(super) const CAR_FRONT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M184,176h40v24a8,8,0,0,1-8,8H192a8,8,0,0,1-8-8ZM32,200a8,8,0,0,0,8,8H64a8,8,0,0,0,8-8V176H32ZM194.11,44.75A8,8,0,0,0,186.8,40H69.2a8,8,0,0,0-7.31,4.75L32,112H224Z"/><path fill="currentColor" d="M240,104H229.2L201.42,41.5A16,16,0,0,0,186.8,32H69.2a16,16,0,0,0-14.62,9.5L26.8,104H16a8,8,0,0,0,0,16h8v80a16,16,0,0,0,16,16H64a16,16,0,0,0,16-16V184h96v16a16,16,0,0,0,16,16h24a16,16,0,0,0,16-16V120h8a8,8,0,0,0,0-16ZM69.2,48H186.8l24.89,56H44.31ZM64,200H40V184H64Zm128,0V184h24v16Zm24-32H40V120H216ZM56,144a8,8,0,0,1,8-8H80a8,8,0,0,1,0,16H64A8,8,0,0,1,56,144Zm112,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H176A8,8,0,0,1,168,144Z"/></g>"#;

/// A name tag.
/// Phosphor's `tag-simple`.
pub(super) const NAME_TAG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,128l-45.62,68.44a8,8,0,0,1-6.66,3.56H40a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H187.72a8,8,0,0,1,6.66,3.56Z"/><path fill="currentColor" d="M246.66,123.56,201,55.13A15.94,15.94,0,0,0,187.72,48H40A16,16,0,0,0,24,64V192a16,16,0,0,0,16,16H187.72A16,16,0,0,0,201,200.88l45.63-68.44A8,8,0,0,0,246.66,123.56ZM187.72,192H40V64H187.72l42.66,64Z"/></g>"#;

/// A calendar.
/// Phosphor's `calendar-blank`.
pub(super) const CALENDAR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,48V88H40V48a8,8,0,0,1,8-8H208A8,8,0,0,1,216,48Z"/><path fill="currentColor" d="M208,32H184V24a8,8,0,0,0-16,0v8H88V24a8,8,0,0,0-16,0v8H48A16,16,0,0,0,32,48V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32ZM72,48v8a8,8,0,0,0,16,0V48h80v8a8,8,0,0,0,16,0V48h24V80H48V48ZM208,208H48V96H208V208Z"/></g>"#;

/// A fuel pump.
/// Phosphor's `gas-pump`.
pub(super) const FUEL_PUMP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M168,56V216H56V56A16,16,0,0,1,72,40h80A16,16,0,0,1,168,56Z"/><path fill="currentColor" d="M241,69.66,221.66,50.34a8,8,0,0,0-11.32,11.32L229.66,81A8,8,0,0,1,232,86.63V168a8,8,0,0,1-16,0V128a24,24,0,0,0-24-24H176V56a24,24,0,0,0-24-24H72A24,24,0,0,0,48,56V208H32a8,8,0,0,0,0,16H192a8,8,0,0,0,0-16H176V120h16a8,8,0,0,1,8,8v40a24,24,0,0,0,48,0V86.63A23.85,23.85,0,0,0,241,69.66ZM64,208V56a8,8,0,0,1,8-8h80a8,8,0,0,1,8,8V208Zm80-96a8,8,0,0,1-8,8H88a8,8,0,0,1,0-16h48A8,8,0,0,1,144,112Z"/></g>"#;

/// A gear lever's H.
/// Phosphor's `joystick`.
pub(super) const GEAR_LEVER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,160v48a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V160a8,8,0,0,1,8-8H208A8,8,0,0,1,216,160ZM128,88A32,32,0,1,0,96,56,32,32,0,0,0,128,88Z"/><path fill="currentColor" d="M208,144H136V95.19a40,40,0,1,0-16,0V144H48a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V160A16,16,0,0,0,208,144ZM104,56a24,24,0,1,1,24,24A24,24,0,0,1,104,56ZM208,208H48V160H208v48Zm-40-96h32a8,8,0,0,1,0,16H168a8,8,0,0,1,0-16Z"/></g>"#;

/// A car from the side.
/// Phosphor's `car-profile`.
pub(super) const CAR_SIDE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M88,176a24,24,0,1,1-24-24A24,24,0,0,1,88,176Zm104-24a24,24,0,1,0,24,24A24,24,0,0,0,192,152ZM162.34,74.34A8,8,0,0,0,156.69,72H44.28a8,8,0,0,0-6.66,3.56L8,120H208Z"/><path fill="currentColor" d="M240,112H211.31L168,68.69A15.86,15.86,0,0,0,156.69,64H44.28A16,16,0,0,0,31,71.12L1.34,115.56A8.07,8.07,0,0,0,0,120v48a16,16,0,0,0,16,16H33a32,32,0,0,0,62,0h66a32,32,0,0,0,62,0h17a16,16,0,0,0,16-16V128A16,16,0,0,0,240,112ZM44.28,80H156.69l32,32H23ZM64,192a16,16,0,1,1,16-16A16,16,0,0,1,64,192Zm128,0a16,16,0,1,1,16-16A16,16,0,0,1,192,192Zm48-24H223a32,32,0,0,0-62,0H95a32,32,0,0,0-62,0H16V128H240Z"/></g>"#;

/// A speedometer.
/// Phosphor's `gauge`.
pub(super) const SPEEDOMETER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,152v24a8,8,0,0,1-8,8H32a8,8,0,0,1-8-8V153.13C24,95.65,70.15,48.2,127.63,48A104,104,0,0,1,232,152Z"/><path fill="currentColor" d="M207.06,72.67A111.24,111.24,0,0,0,128,40h-.4C66.07,40.21,16,91,16,153.13V176a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V152A111.25,111.25,0,0,0,207.06,72.67ZM224,176H119.71l54.76-75.3a8,8,0,0,0-12.94-9.42L99.92,176H32V153.13c0-3.08.15-6.12.43-9.13H56a8,8,0,0,0,0-16H35.27c10.32-38.86,44-68.24,84.73-71.66V80a8,8,0,0,0,16,0V56.33A96.14,96.14,0,0,1,221,128H200a8,8,0,0,0,0,16h23.67c.21,2.65.33,5.31.33,8Z"/></g>"#;

/// A paint palette: a colour among the details, and the accent the visitor picks (the menu's
/// "Ngjyra", base.html).
/// Phosphor's `palette`.
pub const PALETTE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,127.17a96.48,96.48,0,0,1-2.39,22.18A24,24,0,0,1,198.21,168H152a24,24,0,0,0-24,24,24,24,0,0,1-32,22.61C58.73,201.44,32,169.81,32,128a96,96,0,0,1,95-96C179.84,31.47,223.55,74.35,224,127.17Z"/><path fill="currentColor" d="M200.77,53.89A103.27,103.27,0,0,0,128,24h-1.07A104,104,0,0,0,24,128c0,43,26.58,79.06,69.36,94.17A32,32,0,0,0,136,192a16,16,0,0,1,16-16h46.21a31.81,31.81,0,0,0,31.2-24.88,104.43,104.43,0,0,0,2.59-24A103.28,103.28,0,0,0,200.77,53.89Zm13,93.71A15.89,15.89,0,0,1,198.21,160H152a32,32,0,0,0-32,32,16,16,0,0,1-21.31,15.07C62.49,194.3,40,164,40,128a88,88,0,0,1,87.09-88h.9a88.35,88.35,0,0,1,88,87.25A88.86,88.86,0,0,1,213.81,147.6ZM140,76a12,12,0,1,1-12-12A12,12,0,0,1,140,76ZM96,100A12,12,0,1,1,84,88,12,12,0,0,1,96,100Zm0,56a12,12,0,1,1-12-12A12,12,0,0,1,96,156Zm88-56a12,12,0,1,1-12-12A12,12,0,0,1,184,100Z"/></g>"#;

/// A map pin.
/// Phosphor's `map-pin`.
pub(super) const PIN: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,24a80,80,0,0,0-80,80c0,72,80,128,80,128s80-56,80-128A80,80,0,0,0,128,24Zm0,112a32,32,0,1,1,32-32A32,32,0,0,1,128,136Z"/><path fill="currentColor" d="M128,64a40,40,0,1,0,40,40A40,40,0,0,0,128,64Zm0,64a24,24,0,1,1,24-24A24,24,0,0,1,128,128Zm0-112a88.1,88.1,0,0,0-88,88c0,31.4,14.51,64.68,42,96.25a254.19,254.19,0,0,0,41.45,38.3,8,8,0,0,0,9.18,0A254.19,254.19,0,0,0,174,200.25c27.45-31.57,42-64.85,42-96.25A88.1,88.1,0,0,0,128,16Zm0,206c-16.53-13-72-60.75-72-118a72,72,0,0,1,144,0C200,161.23,144.53,209,128,222Z"/></g>"#;

/// A house.
/// Phosphor's `house`.
const HOUSE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,120v96H152V152H104v64H40V120a8,8,0,0,1,2.34-5.66l80-80a8,8,0,0,1,11.32,0l80,80A8,8,0,0,1,216,120Z"/><path fill="currentColor" d="M219.31,108.68l-80-80a16,16,0,0,0-22.62,0l-80,80A15.87,15.87,0,0,0,32,120v96a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V160h32v56a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V120A15.87,15.87,0,0,0,219.31,108.68ZM208,208H160V152a8,8,0,0,0-8-8H104a8,8,0,0,0-8,8v56H48V120l80-80,80,80Z"/></g>"#;

/// A sofa.
/// Phosphor's `couch`.
const SOFA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,120v48a8,8,0,0,1-8,8H24a8,8,0,0,1-8-8V120a8,8,0,0,1,8-8V72a8,8,0,0,1,8-8H224a8,8,0,0,1,8,8v40A8,8,0,0,1,240,120Z"/><path fill="currentColor" d="M240,106.17V72a16,16,0,0,0-16-16H32A16,16,0,0,0,16,72v34.17A16,16,0,0,0,8,120v48a16,16,0,0,0,16,16h8v16a8,8,0,0,0,16,0V184H208v16a8,8,0,0,0,16,0V184h8a16,16,0,0,0,16-16V120A16,16,0,0,0,240,106.17ZM224,104h-8a16,16,0,0,0-16,16v16H136V72h88ZM120,72v64H56V120a16,16,0,0,0-16-16H32V72Zm112,96H24V120H40v24a8,8,0,0,0,8,8H208a8,8,0,0,0,8-8V120h16Z"/></g>"#;

/// A paw.
/// Phosphor's `paw-print`.
const PAW: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,108a20,20,0,1,1-20-20A20,20,0,0,1,232,108ZM64,108a20,20,0,1,0-20,20A20,20,0,0,0,64,108ZM92,80A20,20,0,1,0,72,60,20,20,0,0,0,92,80Zm72,0a20,20,0,1,0-20-20A20,20,0,0,0,164,80Zm19.24,75.85A43.46,43.46,0,0,1,162.57,130a36,36,0,0,0-69.14,0,43.49,43.49,0,0,1-20.67,25.9,32,32,0,0,0,27.73,57.62,72.49,72.49,0,0,1,55,0,32,32,0,0,0,27.73-57.62Z"/><path fill="currentColor" d="M212,80a28,28,0,1,0,28,28A28,28,0,0,0,212,80Zm0,40a12,12,0,1,1,12-12A12,12,0,0,1,212,120ZM72,108a28,28,0,1,0-28,28A28,28,0,0,0,72,108ZM44,120a12,12,0,1,1,12-12A12,12,0,0,1,44,120ZM92,88A28,28,0,1,0,64,60,28,28,0,0,0,92,88Zm0-40A12,12,0,1,1,80,60,12,12,0,0,1,92,48Zm72,40a28,28,0,1,0-28-28A28,28,0,0,0,164,88Zm0-40a12,12,0,1,1-12,12A12,12,0,0,1,164,48Zm23.12,100.86a35.3,35.3,0,0,1-16.87-21.14,44,44,0,0,0-84.5,0A35.25,35.25,0,0,1,69,148.82,40,40,0,0,0,88,224a39.48,39.48,0,0,0,15.52-3.13,64.09,64.09,0,0,1,48.87,0,40,40,0,0,0,34.73-72ZM168,208a24,24,0,0,1-9.45-1.93,80.14,80.14,0,0,0-61.19,0,24,24,0,0,1-20.71-43.26,51.22,51.22,0,0,0,24.46-30.67,28,28,0,0,1,53.78,0,51.27,51.27,0,0,0,24.53,30.71A24,24,0,0,1,168,208Z"/></g>"#;

/// A motorbike.
/// Phosphor's `motorcycle`.
const MOTORBIKE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,160a32,32,0,1,1-32-32A32,32,0,0,1,248,160ZM40,128a32,32,0,1,0,32,32A32,32,0,0,0,40,128Z"/><path fill="currentColor" d="M216,120a41,41,0,0,0-6.6.55l-5.82-15.14A55.64,55.64,0,0,1,216,104a8,8,0,0,0,0-16H196.88L183.47,53.13A8,8,0,0,0,176,48H144a8,8,0,0,0,0,16h26.51l9.23,24H152c-18.5,0-33.5,4.31-43.37,12.46a16,16,0,0,1-16.76,2.07C81.29,97.72,31.13,77.33,26.71,75.6L21,73.36A17.74,17.74,0,0,0,16,72a8,8,0,0,0-2.87,15.46h0c.46.18,47.19,18.3,72.13,29.63a32.15,32.15,0,0,0,33.56-4.29c4.86-4,14.57-8.8,33.19-8.8h18.82a71.74,71.74,0,0,0-24.17,36.59A15.86,15.86,0,0,1,131.32,152H79.2a40,40,0,1,0,0,16h52.12a31.91,31.91,0,0,0,30.74-23.1,56,56,0,0,1,26.59-33.72l5.82,15.13A40,40,0,1,0,216,120ZM40,168H62.62a24,24,0,1,1,0-16H40a8,8,0,0,0,0,16Zm176,16a24,24,0,0,1-15.58-42.23l8.11,21.1a8,8,0,1,0,14.94-5.74L215.35,136l.65,0a24,24,0,0,1,0,48Z"/></g>"#;

/// A delivery truck.
/// Phosphor's `truck`.
const TRUCK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,120v64a8,8,0,0,1-8,8H216a24,24,0,0,0-48,0H104a24,24,0,0,0-48,0H32a8,8,0,0,1-8-8V144H184V120Z"/><path fill="currentColor" d="M255.42,117l-14-35A15.93,15.93,0,0,0,226.58,72H192V64a8,8,0,0,0-8-8H32A16,16,0,0,0,16,72V184a16,16,0,0,0,16,16H49a32,32,0,0,0,62,0h50a32,32,0,0,0,62,0h17a16,16,0,0,0,16-16V120A7.94,7.94,0,0,0,255.42,117ZM192,88h34.58l9.6,24H192ZM32,72H176v64H32ZM80,208a16,16,0,1,1,16-16A16,16,0,0,1,80,208Zm81-24H111a32,32,0,0,0-62,0H32V152H176v12.31A32.11,32.11,0,0,0,161,184Zm31,24a16,16,0,1,1,16-16A16,16,0,0,1,192,208Zm48-24H223a32.06,32.06,0,0,0-31-24V128h48Z"/></g>"#;

/// A tyre on its rim.
/// Phosphor's `tire`.
const TYRE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M164,224H92c-28.72,0-52-43-52-96S63.28,32,92,32h72c-28.72,0-52,43-52,96S135.28,224,164,224Zm4-136c-8.84,0-16,17.91-16,40s7.16,40,16,40,16-17.91,16-40S176.84,88,168,88Z"/><path fill="currentColor" d="M149.26,159.26C155.09,173.82,163.5,176,168,176s12.91-2.18,18.74-16.74c3.39-8.48,5.26-19.58,5.26-31.26s-1.87-22.78-5.26-31.26C180.91,82.18,172.5,80,168,80s-12.91,2.18-18.74,16.74C145.87,105.22,144,116.32,144,128S145.87,150.78,149.26,159.26ZM168,96.2c2.62,2.06,8,13,8,31.8s-5.38,29.74-8,31.8c-2.62-2.06-8-13-8-31.8S165.38,98.26,168,96.2ZM232,216H196.41C213.12,197.73,224,165.47,224,128c0-58.32-26.35-104-60-104H92C58.35,24,32,69.68,32,128S58.35,232,92,232H232a8,8,0,0,0,0-16ZM193.74,63.93C202.93,80.91,208,103.67,208,128s-5.07,47.09-14.26,64.07C185.38,207.5,174.82,216,164,216s-21.38-8.5-29.74-23.93C125.07,175.09,120,152.33,120,128s5.07-47.09,14.26-64.07C142.62,48.5,153.18,40,164,40S185.38,48.5,193.74,63.93ZM48,128c0-2.5.07-5,.17-7.44L80,97.83l24.43,17.45c-.28,4.16-.43,8.41-.43,12.72a179.89,179.89,0,0,0,3.07,33.5l-22.42-16a8,8,0,0,0-9.3,0l-23.74,17A161,161,0,0,1,48,128ZM62.26,63.93C70.62,48.5,81.18,40,92,40h39.59c-11.9,13-20.84,33.12-25,57.16L84.65,81.49a8,8,0,0,0-9.3,0L50.49,99.25C52.85,86,56.83,74,62.26,63.93Zm0,128.14a100.08,100.08,0,0,1-5.94-13.32L80,161.83l33.94,24.24c4.6,12,10.6,22.22,17.65,29.93H92C81.18,216,70.62,207.5,62.26,192.07Z"/></g>"#;

/// A lightning bolt.
/// Phosphor's `lightning`.
const BOLT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M96,240l16-80L48,136,160,16,144,96l64,24Z"/><path fill="currentColor" d="M215.79,118.17a8,8,0,0,0-5-5.66L153.18,90.9l14.66-73.33a8,8,0,0,0-13.69-7l-112,120a8,8,0,0,0,3,13l57.63,21.61L88.16,238.43a8,8,0,0,0,13.69,7l112-120A8,8,0,0,0,215.79,118.17ZM109.37,214l10.47-52.38a8,8,0,0,0-5-9.06L62,132.71l84.62-90.66L136.16,94.43a8,8,0,0,0,5,9.06l52.8,19.8Z"/></g>"#;

/// A bed.
/// Phosphor's `bed`.
const BED: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,112v56H112V80H216A32,32,0,0,1,248,112Z"/><path fill="currentColor" d="M216,72H32V48a8,8,0,0,0-16,0V208a8,8,0,0,0,16,0V176H240v32a8,8,0,0,0,16,0V112A40,40,0,0,0,216,72ZM32,88h72v72H32Zm88,72V88h96a24,24,0,0,1,24,24v48Z"/></g>"#;

/// An office tower.
/// Phosphor's `building-office`.
const OFFICE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,40V216H136V160H88v56H48V40Z"/><path fill="currentColor" d="M248,208H232V96a8,8,0,0,0,0-16H184V48a8,8,0,0,0,0-16H40a8,8,0,0,0,0,16V208H24a8,8,0,0,0,0,16H248a8,8,0,0,0,0-16ZM216,96V208H184V96ZM56,48H168V208H144V160a8,8,0,0,0-8-8H88a8,8,0,0,0-8,8v48H56Zm72,160H96V168h32ZM72,80a8,8,0,0,1,8-8H96a8,8,0,0,1,0,16H80A8,8,0,0,1,72,80Zm48,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H128A8,8,0,0,1,120,80ZM72,120a8,8,0,0,1,8-8H96a8,8,0,0,1,0,16H80A8,8,0,0,1,72,120Zm48,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H128A8,8,0,0,1,120,120Z"/></g>"#;

/// A phone.
/// Phosphor's `device-mobile`.
const PHONE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,56V200H64V56Z"/><path fill="currentColor" d="M176,16H80A24,24,0,0,0,56,40V216a24,24,0,0,0,24,24h96a24,24,0,0,0,24-24V40A24,24,0,0,0,176,16ZM72,64H184V192H72Zm8-32h96a8,8,0,0,1,8,8v8H72V40A8,8,0,0,1,80,32Zm96,192H80a8,8,0,0,1-8-8v-8H184v8A8,8,0,0,1,176,224Z"/></g>"#;

/// A laptop.
/// Phosphor's `laptop`.
const LAPTOP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,72V176H40V72A16,16,0,0,1,56,56H200A16,16,0,0,1,216,72Z"/><path fill="currentColor" d="M232,168h-8V72a24,24,0,0,0-24-24H56A24,24,0,0,0,32,72v96H24a8,8,0,0,0-8,8v16a24,24,0,0,0,24,24H216a24,24,0,0,0,24-24V176A8,8,0,0,0,232,168ZM48,72a8,8,0,0,1,8-8H200a8,8,0,0,1,8,8v96H48ZM224,192a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8v-8H224ZM152,88a8,8,0,0,1-8,8H112a8,8,0,0,1,0-16h32A8,8,0,0,1,152,88Z"/></g>"#;

/// A screen on its stand.
/// Phosphor's `monitor`.
const SCREEN: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,64V176a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V64A16,16,0,0,1,48,48H208A16,16,0,0,1,224,64Z"/><path fill="currentColor" d="M208,40H48A24,24,0,0,0,24,64V176a24,24,0,0,0,24,24H208a24,24,0,0,0,24-24V64A24,24,0,0,0,208,40Zm8,136a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H208a8,8,0,0,1,8,8Zm-48,48a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,224Z"/></g>"#;

/// A tablet.
/// Phosphor's `device-tablet`.
const TABLET: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,64V192H48V64Z"/><path fill="currentColor" d="M192,24H64A24,24,0,0,0,40,48V208a24,24,0,0,0,24,24H192a24,24,0,0,0,24-24V48A24,24,0,0,0,192,24ZM56,72H200V184H56Zm8-32H192a8,8,0,0,1,8,8v8H56V48A8,8,0,0,1,64,40ZM192,216H64a8,8,0,0,1-8-8v-8H200v8A8,8,0,0,1,192,216Z"/></g>"#;

/// A television and its aerial.
/// Phosphor's `television`.
const TV: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M152,72V208H40a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8Z"/><path fill="currentColor" d="M216,64H147.31l34.35-34.34a8,8,0,1,0-11.32-11.32L128,60.69,85.66,18.34A8,8,0,0,0,74.34,29.66L108.69,64H40A16,16,0,0,0,24,80V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V80A16,16,0,0,0,216,64ZM40,80H144V200H40ZM216,200H160V80h56V200Zm-16-84a12,12,0,1,1-12-12A12,12,0,0,1,200,116Zm0,48a12,12,0,1,1-12-12A12,12,0,0,1,200,164Z"/></g>"#;

/// A game controller.
/// Phosphor's `game-controller`.
const GAMEPAD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216.86,207.57a28,28,0,0,1-24.66-7.77L150.09,152H172a51.94,51.94,0,0,0,51.2-61h0l16.36,84.17A28,28,0,0,1,216.86,207.57Z"/><path fill="currentColor" d="M176,112H152a8,8,0,0,1,0-16h24a8,8,0,0,1,0,16ZM104,96H96V88a8,8,0,0,0-16,0v8H72a8,8,0,0,0,0,16h8v8a8,8,0,0,0,16,0v-8h8a8,8,0,0,0,0-16ZM241.48,200.65a36,36,0,0,1-54.94,4.81c-.12-.12-.24-.24-.35-.37L146.48,160h-37L69.81,205.09l-.35.37A36.08,36.08,0,0,1,44,216,36,36,0,0,1,8.56,173.75a.68.68,0,0,1,0-.14L24.93,89.52A59.88,59.88,0,0,1,83.89,40H172a60.08,60.08,0,0,1,59,49.25c0,.06,0,.12,0,.18l16.37,84.17a.68.68,0,0,1,0,.14A35.74,35.74,0,0,1,241.48,200.65ZM172,144a44,44,0,0,0,0-88H83.89A43.9,43.9,0,0,0,40.68,92.37l0,.13L24.3,176.59A20,20,0,0,0,58,194.3l41.92-47.59a8,8,0,0,1,6-2.71Zm59.7,32.59-8.74-45A60,60,0,0,1,172,160h-4.2L198,194.31a20.09,20.09,0,0,0,17.46,5.39,20,20,0,0,0,16.23-23.11Z"/></g>"#;

/// A camera.
/// Phosphor's `camera`.
pub(super) const CAMERA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,64H176L160,40H96L80,64H48A16,16,0,0,0,32,80V192a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V80A16,16,0,0,0,208,64ZM128,168a36,36,0,1,1,36-36A36,36,0,0,1,128,168Z"/><path fill="currentColor" d="M208,56H180.28L166.65,35.56A8,8,0,0,0,160,32H96a8,8,0,0,0-6.65,3.56L75.71,56H48A24,24,0,0,0,24,80V192a24,24,0,0,0,24,24H208a24,24,0,0,0,24-24V80A24,24,0,0,0,208,56Zm8,136a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8H80a8,8,0,0,0,6.66-3.56L100.28,48h55.43l13.63,20.44A8,8,0,0,0,176,72h32a8,8,0,0,1,8,8ZM128,88a44,44,0,1,0,44,44A44.05,44.05,0,0,0,128,88Zm0,72a28,28,0,1,1,28-28A28,28,0,0,1,128,160Z"/></g>"#;

/// Headphones.
/// Phosphor's `headphones`.
const HEADPHONES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M80,144v40a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V128H64A16,16,0,0,1,80,144Zm112-16a16,16,0,0,0-16,16v40a16,16,0,0,0,16,16h16a16,16,0,0,0,16-16V128Z"/><path fill="currentColor" d="M201.89,54.66A104.08,104.08,0,0,0,24,128v56a24,24,0,0,0,24,24H64a24,24,0,0,0,24-24V144a24,24,0,0,0-24-24H40.36A88,88,0,0,1,128,40h.67a87.71,87.71,0,0,1,87,80H192a24,24,0,0,0-24,24v40a24,24,0,0,0,24,24h16a24,24,0,0,0,24-24V128A103.41,103.41,0,0,0,201.89,54.66ZM64,136a8,8,0,0,1,8,8v40a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V136Zm152,48a8,8,0,0,1-8,8H192a8,8,0,0,1-8-8V144a8,8,0,0,1,8-8h24Z"/></g>"#;

/// A chair.
/// Phosphor's `armchair`.
const CHAIR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,128a32,32,0,0,1-32,32v40a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V160a32,32,0,0,1,0-64V72A32,32,0,0,1,80,40h96a32,32,0,0,1,32,32V96A32,32,0,0,1,240,128Z"/><path fill="currentColor" d="M216,88.8V72a40,40,0,0,0-40-40H80A40,40,0,0,0,40,72V88.8a40,40,0,0,0,0,78.4V200a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V167.2a40,40,0,0,0,0-78.4ZM80,48h96a24,24,0,0,1,24,24V88.8A40.07,40.07,0,0,0,168,128H88A40.07,40.07,0,0,0,56,88.8V72A24,24,0,0,1,80,48ZM208.39,152H208a8,8,0,0,0-8,8v40H56V160a8,8,0,0,0-8-8h-.39A24,24,0,1,1,72,128v40a8,8,0,0,0,16,0V144h80v24a8,8,0,0,0,16,0V128a24,24,0,1,1,24.39,24Z"/></g>"#;

/// A washing machine.
/// Phosphor's `washing-machine`.
const WASHING_MACHINE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M184,128a56,56,0,1,1-56-56A56,56,0,0,1,184,128Z"/><path fill="currentColor" d="M208,32H48A16,16,0,0,0,32,48V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Zm0,176H48V48H208V208ZM128,64a64,64,0,1,0,64,64A64.07,64.07,0,0,0,128,64Zm0,112a48,48,0,1,1,48-48A48.05,48.05,0,0,1,128,176ZM200,68a12,12,0,1,1-12-12A12,12,0,0,1,200,68Zm-74.34,49.66-16,16a8,8,0,0,1-11.32-11.32l16-16a8,8,0,0,1,11.32,11.32Zm32-3.32a8,8,0,0,1,0,11.32l-32,32a8,8,0,0,1-11.32-11.32l32-32A8,8,0,0,1,157.66,114.34Z"/></g>"#;

/// A sprout, two leaves.
/// Phosphor's `plant`.
const SPROUT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M138.54,149.46C106.62,96.25,149.18,43.05,239.63,48.37,245,138.82,191.75,181.39,138.54,149.46ZM16.26,88.26c-3.8,64.61,34.21,95,72.21,72.21C111.27,122.47,80.87,84.46,16.26,88.26Z"/><path fill="currentColor" d="M247.63,47.89a8,8,0,0,0-7.52-7.52c-51.76-3-93.32,12.74-111.18,42.22-11.8,19.48-11.78,43.16-.16,65.74a71.37,71.37,0,0,0-14.17,26.95L98.33,159c7.82-16.33,7.52-33.36-1-47.49C84.09,89.73,53.62,78,15.79,80.27a8,8,0,0,0-7.52,7.52c-2.23,37.83,9.46,68.3,31.25,81.5A45.82,45.82,0,0,0,63.44,176,54.58,54.58,0,0,0,87,170.33l25,25V224a8,8,0,0,0,16,0V194.51a55.61,55.61,0,0,1,12.27-35,73.91,73.91,0,0,0,33.31,8.4,60.9,60.9,0,0,0,31.83-8.86C234.89,141.21,250.67,99.65,247.63,47.89ZM86.06,146.74l-24.41-24.4a8,8,0,0,0-11.31,11.31l24.41,24.41c-9.61,3.18-18.93,2.39-26.94-2.46C32.47,146.31,23.79,124.32,24,96c28.31-.25,50.31,8.47,59.6,23.81C88.45,127.82,89.24,137.14,86.06,146.74Zm111.06-1.36c-13.4,8.11-29.15,8.73-45.15,2l53.69-53.7a8,8,0,0,0-11.31-11.32L140.65,136c-6.76-16-6.15-31.76,2-45.15,13.94-23,47-35.8,89.33-34.83C232.94,98.34,220.14,131.44,197.12,145.38Z"/></g>"#;

/// A table lamp.
/// Phosphor's `lamp`.
const LAMP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,152H16L64,40H192Z"/><path fill="currentColor" d="M247.35,148.85l-48-112A8,8,0,0,0,192,32H64a8,8,0,0,0-7.35,4.85l-48,112A8,8,0,0,0,16,160H120v48H96a8,8,0,0,0,0,16h64a8,8,0,0,0,0-16H136V160h56v32a8,8,0,0,0,16,0V160h32a8,8,0,0,0,7.35-11.15ZM28.13,144,69.28,48H186.72l41.15,96Z"/></g>"#;

/// A fork and a knife.
/// Phosphor's `fork-knife`.
const FORK_AND_KNIFE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,40V168H152S152,64,208,40Z"/><path fill="currentColor" d="M72,88V40a8,8,0,0,1,16,0V88a8,8,0,0,1-16,0ZM216,40V224a8,8,0,0,1-16,0V176H152a8,8,0,0,1-8-8,268.75,268.75,0,0,1,7.22-56.88c9.78-40.49,28.32-67.63,53.63-78.47A8,8,0,0,1,216,40ZM200,53.9c-32.17,24.57-38.47,84.42-39.7,106.1H200ZM119.89,38.69a8,8,0,1,0-15.78,2.63L112,88.63a32,32,0,0,1-64,0l7.88-47.31a8,8,0,1,0-15.78-2.63l-8,48A8.17,8.17,0,0,0,32,88a48.07,48.07,0,0,0,40,47.32V224a8,8,0,0,0,16,0V135.32A48.07,48.07,0,0,0,128,88a8.17,8.17,0,0,0-.11-1.31Z"/></g>"#;

/// A medical cross.
/// Phosphor's `stethoscope`.
const MEDICAL_CROSS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,160a32,32,0,1,1-32-32A32,32,0,0,1,240,160Z"/><path fill="currentColor" d="M220,160a12,12,0,1,1-12-12A12,12,0,0,1,220,160Zm-4.55,39.29A48.08,48.08,0,0,1,168,240H144a48.05,48.05,0,0,1-48-48V151.49A64,64,0,0,1,40,88V40a8,8,0,0,1,8-8H72a8,8,0,0,1,0,16H56V88a48,48,0,0,0,48.64,48c26.11-.34,47.36-22.25,47.36-48.83V48H136a8,8,0,0,1,0-16h24a8,8,0,0,1,8,8V87.17c0,32.84-24.53,60.29-56,64.31V192a32,32,0,0,0,32,32h24a32.06,32.06,0,0,0,31.22-25,40,40,0,1,1,16.23.27ZM232,160a24,24,0,1,0-24,24A24,24,0,0,0,232,160Z"/></g>"#;

/// A bus from the front.
/// Phosphor's `bus`.
const BUS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M48,184H88v24a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8Zm120,24a8,8,0,0,0,8,8h24a8,8,0,0,0,8-8V184H168ZM48,72v40H208V72Z"/><path fill="currentColor" d="M184,32H72A32,32,0,0,0,40,64V208a16,16,0,0,0,16,16H80a16,16,0,0,0,16-16V192h64v16a16,16,0,0,0,16,16h24a16,16,0,0,0,16-16V64A32,32,0,0,0,184,32ZM56,176V120H200v56Zm0-96H200v24H56ZM72,48H184a16,16,0,0,1,16,16H56A16,16,0,0,1,72,48Zm8,160H56V192H80Zm96,0V192h24v16Zm-72-60a12,12,0,1,1-12-12A12,12,0,0,1,104,148Zm72,0a12,12,0,1,1-12-12A12,12,0,0,1,176,148Zm72-68v24a8,8,0,0,1-16,0V80a8,8,0,0,1,16,0ZM24,80v24a8,8,0,0,1-16,0V80a8,8,0,0,1,16,0Z"/></g>"#;

/// An open book.
/// Phosphor's `book-open`.
const OPEN_BOOK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,56V200H160a32,32,0,0,0-32,32,32,32,0,0,0-32-32H24V56H96a32,32,0,0,1,32,32,32,32,0,0,1,32-32Z"/><path fill="currentColor" d="M232,48H160a40,40,0,0,0-32,16A40,40,0,0,0,96,48H24a8,8,0,0,0-8,8V200a8,8,0,0,0,8,8H96a24,24,0,0,1,24,24,8,8,0,0,0,16,0,24,24,0,0,1,24-24h72a8,8,0,0,0,8-8V56A8,8,0,0,0,232,48ZM96,192H32V64H96a24,24,0,0,1,24,24V200A39.81,39.81,0,0,0,96,192Zm128,0H160a39.81,39.81,0,0,0-24,8V88a24,24,0,0,1,24-24h64Z"/></g>"#;

/// A jacket, open at the collar.
/// Phosphor's `hoodie`.
const JACKET: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M231.72,131.51,210.28,210.1a8,8,0,0,1-7.71,5.9H176a8,8,0,0,1-8-8V184H88v24a8,8,0,0,1-8,8H53.43a8,8,0,0,1-7.71-5.9L24.28,131.51A8,8,0,0,1,25.34,125L72,55l56,33,56-33,46.66,70A8,8,0,0,1,231.72,131.51Z"/><path fill="currentColor" d="M237.31,120.53,183,39.12A16,16,0,0,0,169.73,32H86.27A16,16,0,0,0,73,39.12L18.69,120.53a16,16,0,0,0-2.13,13.09L38,212.21A16,16,0,0,0,53.43,224H80a16,16,0,0,0,16-16V192h64v16a16,16,0,0,0,16,16h26.57A16,16,0,0,0,218,212.21l21.44-78.59A16,16,0,0,0,237.31,120.53ZM80,176V69l24,14.15V136a8,8,0,0,0,16,0V92.57l3.94,2.32a8,8,0,0,0,8.12,0L136,92.57V128a8,8,0,0,0,16,0V83.14L176,69V176ZM169.73,48l2.92,4.39L128,78.71,83.35,52.39,86.27,48ZM80,208H53.43L32,129.41l32-48V176a16,16,0,0,0,16,16Zm122.57,0H176V192a16,16,0,0,0,16-16V81.41l32,48Z"/></g>"#;

/// A trainer.
/// Phosphor's `sneaker`.
const TRAINER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,167.06V168H32V75.54A8,8,0,0,1,37.27,68L113,40.48a8,8,0,0,1,10,4.27L146.27,99.1a32,32,0,0,0,19.12,17.36l60.73,20.25A32,32,0,0,1,248,167.06Z"/><path fill="currentColor" d="M228.65,129.11l-60.73-20.24a24,24,0,0,1-14.32-13L130.39,41.6s0-.07,0-.1A16,16,0,0,0,110.25,33L34.53,60.49A16.05,16.05,0,0,0,24,75.53V192a16,16,0,0,0,16,16H240a16,16,0,0,0,16-16V167.06A40,40,0,0,0,228.65,129.11ZM40,75.53,115.72,48l7.11,16.63-21.56,7.85A8,8,0,0,0,104,88a7.91,7.91,0,0,0,2.73-.49l22.4-8.14,4.74,11.07-16.6,6A8,8,0,0,0,120,112a7.91,7.91,0,0,0,2.73-.49l17.6-6.4a40.06,40.06,0,0,0,7.68,10l-14.74,5.36A8,8,0,0,0,136,136a8.14,8.14,0,0,0,2.73-.48l28-10.18,56.87,18.95A24,24,0,0,1,238.93,160H40ZM240,192H40V176H240Z"/></g>"#;

/// A dress.
/// Phosphor's `dress`.
const DRESS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,224H56a8,8,0,0,1-7.35-11.15L96,112,73.14,76.12a8,8,0,0,1,0-8.24L96,35.23,103,44a32,32,0,0,0,50,0l7-8.76,22.86,32.65a8,8,0,0,1,0,8.24L160,112l47.34,100.85A8,8,0,0,1,200,224Z"/><path fill="currentColor" d="M214.7,209.7a1.89,1.89,0,0,0-.11-.25l-45.48-96.86,20.5-32.18.11-.18a16,16,0,0,0,0-16.46c-.09-.16-.2-.32-.3-.47L168,32.7V8a8,8,0,0,0-16,0V32.42L146.74,39a24,24,0,0,1-37.48,0L104,32.42V8A8,8,0,0,0,88,8V32.7L66.58,63.3c-.1.15-.21.31-.3.47a16,16,0,0,0,0,16.46l.11.18,20.5,32.18L41.41,209.45a1.89,1.89,0,0,0-.11.25A16,16,0,0,0,56,232H200a16,16,0,0,0,14.71-22.3ZM80,72,96.43,48.57l.33.42a40,40,0,0,0,62.48,0l.33-.42L176,72l-20.38,32H100.39ZM56,216l45.07-96h53.84L200,216Z"/></g>"#;

/// A handbag.
/// Phosphor's `handbag`.
const HANDBAG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M223.92,208H32.08a8,8,0,0,1-8-8.93l14.25-120a8.06,8.06,0,0,1,8-7.07H209.67a8.06,8.06,0,0,1,8,7.07l14.25,120A8,8,0,0,1,223.92,208Z"/><path fill="currentColor" d="M239.89,198.12l-14.26-120a16,16,0,0,0-16-14.12H176a48,48,0,0,0-96,0H46.33a16,16,0,0,0-16,14.12l-14.26,120A16,16,0,0,0,20,210.6a16.13,16.13,0,0,0,12,5.4H223.92A16.13,16.13,0,0,0,236,210.6,16,16,0,0,0,239.89,198.12ZM128,32a32,32,0,0,1,32,32H96A32,32,0,0,1,128,32ZM32,200,46.33,80H80v24a8,8,0,0,0,16,0V80h64v24a8,8,0,0,0,16,0V80h33.75l14.17,120Z"/></g>"#;

/// A wristwatch.
/// Phosphor's `watch`.
const WRISTWATCH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,128a72,72,0,1,1-72-72A72,72,0,0,1,200,128Z"/><path fill="currentColor" d="M208,128a79.94,79.94,0,0,0-32.7-64.47l-6.24-34.38A16,16,0,0,0,153.32,16H102.68A16,16,0,0,0,86.94,29.15L80.7,63.53a79.9,79.9,0,0,0,0,128.94l6.24,34.38A16,16,0,0,0,102.68,240h50.64a16,16,0,0,0,15.74-13.15l6.24-34.38A79.94,79.94,0,0,0,208,128ZM102.68,32h50.64l3.91,21.55a79.75,79.75,0,0,0-58.46,0ZM64,128a64,64,0,1,1,64,64A64.07,64.07,0,0,1,64,128Zm89.32,96H102.68l-3.91-21.55a79.75,79.75,0,0,0,58.46,0ZM120,128V88a8,8,0,0,1,16,0v32h32a8,8,0,0,1,0,16H128A8,8,0,0,1,120,128Z"/></g>"#;

/// Sunglasses.
/// Phosphor's `sunglasses`.
const SUNGLASSES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M32,136h72v28a36,36,0,0,1-72,0Zm120,0v28a36,36,0,0,0,72,0V136Z"/><path fill="currentColor" d="M200,40a8,8,0,0,0,0,16,16,16,0,0,1,16,16v56H40V72A16,16,0,0,1,56,56a8,8,0,0,0,0-16A32,32,0,0,0,24,72v92a44,44,0,0,0,88,0V144h32v20a44,44,0,0,0,88,0V72A32,32,0,0,0,200,40Zm12.63,137.31L179.31,144H216v20A27.8,27.8,0,0,1,212.63,177.31ZM40,164V147.31l41.31,41.32A28,28,0,0,1,40,164Zm56,0a27.8,27.8,0,0,1-3.37,13.31L59.31,144H96Zm64,0V147.31l41.31,41.32A28,28,0,0,1,160,164Z"/></g>"#;

/// A bicycle.
/// Phosphor's `bicycle`.
const BICYCLE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,160a40,40,0,1,1-40-40A40,40,0,0,1,248,160ZM48,120a40,40,0,1,0,40,40A40,40,0,0,0,48,120Z"/><path fill="currentColor" d="M208,112a47.81,47.81,0,0,0-16.93,3.09L165.93,72H192a8,8,0,0,1,8,8,8,8,0,0,0,16,0,24,24,0,0,0-24-24H152a8,8,0,0,0-6.91,12l11.65,20H99.26L82.91,60A8,8,0,0,0,76,56H48a8,8,0,0,0,0,16H71.41L85.12,95.51,69.41,117.06a48.13,48.13,0,1,0,12.92,9.44l11.59-15.9L125.09,164A8,8,0,1,0,138.91,156l-30.32-52h57.48l11.19,19.17A48,48,0,1,0,208,112ZM80,160a32,32,0,1,1-20.21-29.74l-18.25,25a8,8,0,1,0,12.92,9.42l18.25-25A31.88,31.88,0,0,1,80,160Zm128,32a32,32,0,0,1-22.51-54.72L201.09,164A8,8,0,1,0,214.91,156L199.3,129.21A32,32,0,1,1,208,192Z"/></g>"#;

/// A dumbbell.
/// Phosphor's `barbell`.
const DUMBBELL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M96,64V192a8,8,0,0,1-8,8H64a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H88A8,8,0,0,1,96,64Zm96-8H168a8,8,0,0,0-8,8V192a8,8,0,0,0,8,8h24a8,8,0,0,0,8-8V64A8,8,0,0,0,192,56Z"/><path fill="currentColor" d="M248,120h-8V88a16,16,0,0,0-16-16H208V64a16,16,0,0,0-16-16H168a16,16,0,0,0-16,16v56H104V64A16,16,0,0,0,88,48H64A16,16,0,0,0,48,64v8H32A16,16,0,0,0,16,88v32H8a8,8,0,0,0,0,16h8v32a16,16,0,0,0,16,16H48v8a16,16,0,0,0,16,16H88a16,16,0,0,0,16-16V136h48v56a16,16,0,0,0,16,16h24a16,16,0,0,0,16-16v-8h16a16,16,0,0,0,16-16V136h8a8,8,0,0,0,0-16ZM32,168V88H48v80Zm56,24H64V64H88V192Zm104,0H168V64h24V175.83c0,.06,0,.11,0,.17s0,.12,0,.17V192Zm32-24H208V88h16Z"/></g>"#;

/// Two notes.
/// Phosphor's `music-notes`.
const NOTES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,164a28,28,0,1,1-28-28A28,28,0,0,1,208,164ZM52,168a28,28,0,1,0,28,28A28,28,0,0,0,52,168Z"/><path fill="currentColor" d="M212.92,17.69a8,8,0,0,0-6.86-1.45l-128,32A8,8,0,0,0,72,56V166.08A36,36,0,1,0,88,196V110.25l112-28v51.83A36,36,0,1,0,216,164V24A8,8,0,0,0,212.92,17.69ZM52,216a20,20,0,1,1,20-20A20,20,0,0,1,52,216ZM88,93.75V62.25l112-28v31.5ZM180,184a20,20,0,1,1,20-20A20,20,0,0,1,180,184Z"/></g>"#;

/// A tent.
/// Phosphor's `tent`.
const TENT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M136,192H8L72,48Z"/><path fill="currentColor" d="M255.31,188.75l-64-144A8,8,0,0,0,184,40H72a8,8,0,0,0-7.27,4.69.21.21,0,0,0,0,.06l0,.12,0,0L.69,188.75A8,8,0,0,0,8,200H248a8,8,0,0,0,7.31-11.25ZM64,184H20.31L64,85.7Zm16,0V85.7L123.69,184Zm61.2,0L84.31,56H178.8l56.89,128Z"/></g>"#;

/// A snowflake.
/// Phosphor's `snowflake`.
const SNOWFLAKE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M220,128a92,92,0,1,1-92-92A92.1,92.1,0,0,1,220,128Z"/><path fill="currentColor" d="M223.77,150.09a8,8,0,0,1-5.86,9.68l-24.64,6,6.46,24.11a8,8,0,0,1-5.66,9.8A8.25,8.25,0,0,1,192,200a8,8,0,0,1-7.72-5.93l-7.72-28.8L136,141.86v46.83l21.66,21.65a8,8,0,0,1-11.32,11.32L128,203.31l-18.34,18.35a8,8,0,0,1-11.32-11.32L120,188.69V141.86L79.45,165.27l-7.72,28.8A8,8,0,0,1,64,200a8.25,8.25,0,0,1-2.08-.27,8,8,0,0,1-5.66-9.8l6.46-24.11-24.64-6a8,8,0,0,1,3.82-15.54l29.45,7.23L112,128,71.36,104.54l-29.45,7.23A7.85,7.85,0,0,1,40,112a8,8,0,0,1-1.91-15.77l24.64-6L56.27,66.07a8,8,0,0,1,15.46-4.14l7.72,28.8L120,114.14V67.31L98.34,45.66a8,8,0,0,1,11.32-11.32L128,52.69l18.34-18.35a8,8,0,0,1,11.32,11.32L136,67.31v46.83l40.55-23.41,7.72-28.8a8,8,0,0,1,15.46,4.14l-6.46,24.11,24.64,6A8,8,0,0,1,216,112a7.85,7.85,0,0,1-1.91-.23l-29.45-7.23L144,128l40.64,23.46,29.45-7.23A8,8,0,0,1,223.77,150.09Z"/></g>"#;

/// A dog's head, its ears down.
/// Phosphor's `dog`.
const DOG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,120.38V184a32,32,0,0,1-32,32H80a32,32,0,0,1-32-32V120.38L104,48h48Z"/><path fill="currentColor" d="M239.71,125l-16.42-88a16,16,0,0,0-19.61-12.58l-.31.09L150.85,40h-45.7L52.63,24.56l-.31-.09A16,16,0,0,0,32.71,37.05L16.29,125a15.77,15.77,0,0,0,9.12,17.52A16.26,16.26,0,0,0,32.12,144,15.48,15.48,0,0,0,40,141.84V184a40,40,0,0,0,40,40h96a40,40,0,0,0,40-40V141.85a15.5,15.5,0,0,0,7.87,2.16,16.31,16.31,0,0,0,6.72-1.47A15.77,15.77,0,0,0,239.71,125ZM32,128h0L48.43,40,90.5,52.37Zm144,80H136V195.31l13.66-13.65a8,8,0,0,0-11.32-11.32L128,180.69l-10.34-10.35a8,8,0,0,0-11.32,11.32L120,195.31V208H80a24,24,0,0,1-24-24V123.11L107.93,56h40.14L200,123.11V184A24,24,0,0,1,176,208Zm48-80L165.5,52.37,207.57,40,224,128ZM104,140a12,12,0,1,1-12-12A12,12,0,0,1,104,140Zm72,0a12,12,0,1,1-12-12A12,12,0,0,1,176,140Z"/></g>"#;

/// A cat's head, its ears up.
/// Phosphor's `cat`.
const CAT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,48v88c0,48.6-43,88-96,88s-96-39.4-96-88V48a8,8,0,0,1,13.66-5.66L67.6,67.6h0a102.87,102.87,0,0,1,120.8,0h0l21.94-25.24A8,8,0,0,1,224,48Z"/><path fill="currentColor" d="M96,140a12,12,0,1,1-12-12A12,12,0,0,1,96,140Zm76-12a12,12,0,1,0,12,12A12,12,0,0,0,172,128Zm60-80v88c0,52.93-46.65,96-104,96S24,188.93,24,136V48A16,16,0,0,1,51.31,36.69c.14.14.26.27.38.41L69,57a111.22,111.22,0,0,1,118.1,0L204.31,37.1c.12-.14.24-.27.38-.41A16,16,0,0,1,232,48Zm-16,0-21.56,24.8A8,8,0,0,1,183.63,74,88.86,88.86,0,0,0,168,64.75V88a8,8,0,1,1-16,0V59.05a97.43,97.43,0,0,0-16-2.72V88a8,8,0,1,1-16,0V56.33a97.43,97.43,0,0,0-16,2.72V88a8,8,0,1,1-16,0V64.75A88.86,88.86,0,0,0,72.37,74a8,8,0,0,1-10.81-1.17L40,48v88c0,41.66,35.21,76,80,79.67V195.31l-13.66-13.66a8,8,0,0,1,11.32-11.31L128,180.68l10.34-10.34a8,8,0,0,1,11.32,11.31L136,195.31v20.36c44.79-3.69,80-38,80-79.67Z"/></g>"#;

/// A bird.
/// Phosphor's `bird`.
const BIRD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,80,208,96v24a96,96,0,0,1-96,96H24a8,8,0,0,1-6.25-13L104,99.52V76.89c0-28.77,23-52.75,51.74-52.89a52,52,0,0,1,50.59,38.89Z"/><path fill="currentColor" d="M176,68a12,12,0,1,1-12-12A12,12,0,0,1,176,68Zm64,12a8,8,0,0,1-3.56,6.66L216,100.28V120A104.11,104.11,0,0,1,112,224H24a16,16,0,0,1-12.49-26l.1-.12L96,96.63V76.89C96,43.47,122.79,16.16,155.71,16H156a60,60,0,0,1,57.21,41.86l23.23,15.48A8,8,0,0,1,240,80Zm-22.42,0L201.9,69.54a8,8,0,0,1-3.31-4.64A44,44,0,0,0,156,32h-.22C131.64,32.12,112,52.25,112,76.89V99.52a8,8,0,0,1-1.85,5.13L24,208h26.9l70.94-85.12a8,8,0,1,1,12.29,10.24L71.75,208H112a88.1,88.1,0,0,0,88-88V96a8,8,0,0,1,3.56-6.66Z"/></g>"#;

/// A fish.
/// Phosphor's `fish`.
const FISH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M185.81,167.62c-18,13.9-44.17,23.41-81.68,24.31L80.09,248l-16-56.07L8,175.87l56.07-24c.9-37.49,10.41-63.61,24.31-81.65A44,44,0,0,0,136.2,119.8a44,44,0,0,0,49.61,47.82Z"/><path fill="currentColor" d="M168,76a12,12,0,1,1-12-12A12,12,0,0,1,168,76Zm48.72,67.64c-19.37,34.9-55.44,53.76-107.24,56.1l-22,51.41A8,8,0,0,1,80.1,256l-.51,0a8,8,0,0,1-7.19-5.78L57.6,198.39,5.8,183.56a8,8,0,0,1-1-15.05l51.41-22c2.35-51.78,21.21-87.84,56.09-107.22,24.75-13.74,52.74-15.84,71.88-15.18,18.64.64,36,4.27,38.86,6a8,8,0,0,1,2.83,2.83c1.69,2.85,5.33,20.21,6,38.85C232.55,90.89,230.46,118.89,216.72,143.64Zm-55.18,29a52.11,52.11,0,0,1-33.4-44.78A52.09,52.09,0,0,1,83.37,94.47q-10.45,23.79-11.3,57.59a8,8,0,0,1-4.85,7.17L31.83,174.37l34.45,9.86a8,8,0,0,1,5.49,5.5l9.84,34.44,15.16-35.4a8,8,0,0,1,7.17-4.84Q137.71,183.12,161.54,172.64ZM212.42,43.57c-14.15-3-64.1-11-100.3,14.75a81.21,81.21,0,0,0-16,15.07,36,36,0,0,0,39.35,38.44,8,8,0,0,1,8.73,8.73,36,36,0,0,0,38.47,39.34,80.81,80.81,0,0,0,15-16C223.42,107.73,215.42,57.74,212.42,43.57Z"/></g>"#;

/// A bone.
/// Phosphor's `bone`.
const BONE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M225.09,102.44a28,28,0,0,1-35.92,5.3,8,8,0,0,0-10,1.07l-70.38,70.38a8,8,0,0,0-1.07,10,28,28,0,1,1-51.42,10.51,28,28,0,1,1,10.51-51.42,8,8,0,0,0,10-1.07l70.38-70.38a8,8,0,0,0,1.07-10,28,28,0,1,1,51.42-10.51,28,28,0,0,1,25.41,46.12Z"/><path fill="currentColor" d="M231.67,60.89a35.82,35.82,0,0,0-23.82-12.74,36,36,0,1,0-66.37,22.92.25.25,0,0,1,0,.08L71.17,141.51s0,0-.1,0a36,36,0,1,0-22.92,66.37,36,36,0,1,0,66.37-22.92.54.54,0,0,1,0-.08l70.35-70.36s0,0,.1,0a36,36,0,0,0,46.74-53.63ZM219.1,97.16a20,20,0,0,1-25.67,3.8,16,16,0,0,0-19.88,2.19l-70.4,70.4A16,16,0,0,0,101,193.43a20,20,0,1,1-36.75,7.5,8,8,0,0,0-7.91-9.24,8.5,8.5,0,0,0-1.23.1A20,20,0,1,1,62.57,155a16,16,0,0,0,19.88-2.19l70.4-70.4A16,16,0,0,0,155,62.57a20,20,0,1,1,36.75-7.5,8,8,0,0,0,9.14,9.14,20,20,0,0,1,18.17,33Z"/></g>"#;

/// An excavator, its arm and bucket.
/// Phosphor's `bulldozer`.
const EXCAVATOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,176h0a32,32,0,0,1-32,32H40A32,32,0,0,1,8,176H8a32,32,0,0,1,32-32H160A32,32,0,0,1,192,176Z"/><path fill="currentColor" d="M248,200h-8a8,8,0,0,1-8-8V160a8,8,0,0,1,8-8h8a8,8,0,0,0,0-16h-8a24,24,0,0,0-24,24v8H199.2a40.09,40.09,0,0,0-33.71-31.61L129.44,49.85A16,16,0,0,0,114.67,40H24A16,16,0,0,0,8,56v96a40,40,0,0,0,32,64H160a40.07,40.07,0,0,0,39.2-32H216v8a24,24,0,0,0,24,24h8a8,8,0,0,0,0-16ZM148,136H64V56h50.67ZM48,56v80H40a39.72,39.72,0,0,0-16,3.35V56ZM160,200H40a24,24,0,0,1,0-48H160a24,24,0,0,1,0,48Zm8-24a8,8,0,0,1-8,8H40a8,8,0,0,1,0-16H160A8,8,0,0,1,168,176Z"/></g>"#;

/// A tractor.
/// Phosphor's `tractor`.
const TRACTOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,188a28,28,0,1,1-28-28A28,28,0,0,1,240,188ZM68,128a44,44,0,1,0,44,44A44,44,0,0,0,68,128Z"/><path fill="currentColor" d="M240,165.41V134a15.89,15.89,0,0,0-11.4-15.32l-.21-.06L192,108.71V72a8,8,0,0,0-16,0v32.38l-24-6.5V56h8a8,8,0,0,0,0-16H40a8,8,0,0,0,0,16h8V88H40a8,8,0,0,0,0,16H68a68.07,68.07,0,0,1,68,68v12a8,8,0,0,0,8,8h32.23A36,36,0,1,0,240,165.41ZM68,88H64V56h72v66.77A83.92,83.92,0,0,0,68,88Zm84,26.45L224,134v20.1A36,36,0,0,0,178.06,176H152ZM212,208a20,20,0,1,1,20-20A20,20,0,0,1,212,208ZM68,120a52,52,0,1,0,52,52A52.06,52.06,0,0,0,68,120Zm0,88a36,36,0,1,1,36-36A36,36,0,0,1,68,208Zm12-36a12,12,0,1,1-12-12A12,12,0,0,1,80,172Z"/></g>"#;

/// A solar panel on its stand.
/// Phosphor's `solar-panel`.
const SOLAR_PANEL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,216H24l40.7-72H191.3Z"/><path fill="currentColor" d="M32,104a8,8,0,0,1,8-8H56a8,8,0,0,1,0,16H40A8,8,0,0,1,32,104ZM71.43,58.75A8,8,0,0,0,82.75,47.43L71.43,36.12A8,8,0,0,0,60.12,47.43ZM128,40a8,8,0,0,0,8-8V16a8,8,0,0,0-16,0V32A8,8,0,0,0,128,40Zm50.91,21.09a8,8,0,0,0,5.66-2.34l11.31-11.32a8,8,0,0,0-11.31-11.31L173.25,47.43a8,8,0,0,0,5.66,13.66ZM192,104a8,8,0,0,0,8,8h16a8,8,0,0,0,0-16H200A8,8,0,0,0,192,104ZM88,112a8,8,0,0,0,8-8,32,32,0,0,1,64,0,8,8,0,0,0,16,0,48,48,0,0,0-96,0A8,8,0,0,0,88,112ZM238.91,220a8,8,0,0,1-6.91,4H24a8,8,0,0,1-7-11.94l40.69-72a8,8,0,0,1,7-4.06H191.3a8,8,0,0,1,7,4.06l40.69,72A8,8,0,0,1,238.91,220Zm-52.27-68H162.27l3.48,16h29.93Zm-37.26,16-3.48-16H110.1l-3.48,16Zm-46.24,16-5.21,24h60.14l-5.21-24ZM60.32,168H90.25l3.48-16H69.36ZM37.71,208H81.55l5.22-24H51.28Zm180.58,0-13.57-24H169.23l5.22,24Z"/></g>"#;

/// A cog.
/// Phosphor's `gear`.
const COG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M207.86,123.18l16.78-21a99.14,99.14,0,0,0-10.07-24.29l-26.7-3a81,81,0,0,0-6.81-6.81l-3-26.71a99.43,99.43,0,0,0-24.3-10l-21,16.77a81.59,81.59,0,0,0-9.64,0l-21-16.78A99.14,99.14,0,0,0,77.91,41.43l-3,26.7a81,81,0,0,0-6.81,6.81l-26.71,3a99.43,99.43,0,0,0-10,24.3l16.77,21a81.59,81.59,0,0,0,0,9.64l-16.78,21a99.14,99.14,0,0,0,10.07,24.29l26.7,3a81,81,0,0,0,6.81,6.81l3,26.71a99.43,99.43,0,0,0,24.3,10l21-16.77a81.59,81.59,0,0,0,9.64,0l21,16.78a99.14,99.14,0,0,0,24.29-10.07l3-26.7a81,81,0,0,0,6.81-6.81l26.71-3a99.43,99.43,0,0,0,10-24.3l-16.77-21A81.59,81.59,0,0,0,207.86,123.18ZM128,168a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z"/><path fill="currentColor" d="M128,80a48,48,0,1,0,48,48A48.05,48.05,0,0,0,128,80Zm0,80a32,32,0,1,1,32-32A32,32,0,0,1,128,160Zm88-29.84q.06-2.16,0-4.32l14.92-18.64a8,8,0,0,0,1.48-7.06,107.6,107.6,0,0,0-10.88-26.25,8,8,0,0,0-6-3.93l-23.72-2.64q-1.48-1.56-3-3L186,40.54a8,8,0,0,0-3.94-6,107.29,107.29,0,0,0-26.25-10.86,8,8,0,0,0-7.06,1.48L130.16,40Q128,40,125.84,40L107.2,25.11a8,8,0,0,0-7.06-1.48A107.6,107.6,0,0,0,73.89,34.51a8,8,0,0,0-3.93,6L67.32,64.27q-1.56,1.49-3,3L40.54,70a8,8,0,0,0-6,3.94,107.71,107.71,0,0,0-10.87,26.25,8,8,0,0,0,1.49,7.06L40,125.84Q40,128,40,130.16L25.11,148.8a8,8,0,0,0-1.48,7.06,107.6,107.6,0,0,0,10.88,26.25,8,8,0,0,0,6,3.93l23.72,2.64q1.49,1.56,3,3L70,215.46a8,8,0,0,0,3.94,6,107.71,107.71,0,0,0,26.25,10.87,8,8,0,0,0,7.06-1.49L125.84,216q2.16.06,4.32,0l18.64,14.92a8,8,0,0,0,7.06,1.48,107.21,107.21,0,0,0,26.25-10.88,8,8,0,0,0,3.93-6l2.64-23.72q1.56-1.48,3-3L215.46,186a8,8,0,0,0,6-3.94,107.71,107.71,0,0,0,10.87-26.25,8,8,0,0,0-1.49-7.06Zm-16.1-6.5a73.93,73.93,0,0,1,0,8.68,8,8,0,0,0,1.74,5.48l14.19,17.73a91.57,91.57,0,0,1-6.23,15L187,173.11a8,8,0,0,0-5.1,2.64,74.11,74.11,0,0,1-6.14,6.14,8,8,0,0,0-2.64,5.1l-2.51,22.58a91.32,91.32,0,0,1-15,6.23l-17.74-14.19a8,8,0,0,0-5-1.75h-.48a73.93,73.93,0,0,1-8.68,0,8.06,8.06,0,0,0-5.48,1.74L100.45,215.8a91.57,91.57,0,0,1-15-6.23L82.89,187a8,8,0,0,0-2.64-5.1,74.11,74.11,0,0,1-6.14-6.14,8,8,0,0,0-5.1-2.64L46.43,170.6a91.32,91.32,0,0,1-6.23-15l14.19-17.74a8,8,0,0,0,1.74-5.48,73.93,73.93,0,0,1,0-8.68,8,8,0,0,0-1.74-5.48L40.2,100.45a91.57,91.57,0,0,1,6.23-15L69,82.89a8,8,0,0,0,5.1-2.64,74.11,74.11,0,0,1,6.14-6.14A8,8,0,0,0,82.89,69L85.4,46.43a91.32,91.32,0,0,1,15-6.23l17.74,14.19a8,8,0,0,0,5.48,1.74,73.93,73.93,0,0,1,8.68,0,8.06,8.06,0,0,0,5.48-1.74L155.55,40.2a91.57,91.57,0,0,1,15,6.23L173.11,69a8,8,0,0,0,2.64,5.1,74.11,74.11,0,0,1,6.14,6.14,8,8,0,0,0,5.1,2.64l22.58,2.51a91.32,91.32,0,0,1,6.23,15l-14.19,17.74A8,8,0,0,0,199.87,123.66Z"/></g>"#;

/// A rubber stamp.
/// Phosphor's `stamp`.
const STAMP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M159.46,53l-17.78,83H114.32L96.54,53A24,24,0,0,1,120,24h16A24,24,0,0,1,159.46,53Z"/><path fill="currentColor" d="M224,224a8,8,0,0,1-8,8H40a8,8,0,0,1,0-16H216A8,8,0,0,1,224,224Zm0-80v40a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V144a16,16,0,0,1,16-16h56.43L88.72,54.71A32,32,0,0,1,120,16h16a32,32,0,0,1,31.29,38.71L151.57,128H208A16,16,0,0,1,224,144ZM120.79,128h14.42l16.43-76.65A16,16,0,0,0,136,32H120a16,16,0,0,0-15.65,19.35ZM208,184V144H48v40H208Z"/></g>"#;

/// A number plate, its band at the left.
/// Phosphor's `rectangle`.
const NUMBER_PLATE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V200a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z"/><path fill="currentColor" d="M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H40V56H216V200Z"/></g>"#;

/// A leaf.
/// Phosphor's `leaf`.
const LEAF: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M63.81,192.19c-47.89-79.81,16-159.62,151.64-151.64C223.43,176.23,143.62,240.08,63.81,192.19Z"/><path fill="currentColor" d="M223.45,40.07a8,8,0,0,0-7.52-7.52C139.8,28.08,78.82,51,52.82,94a87.09,87.09,0,0,0-12.76,49c.57,15.92,5.21,32,13.79,47.85l-19.51,19.5a8,8,0,0,0,11.32,11.32l19.5-19.51C81,210.73,97.09,215.37,113,215.94q1.67.06,3.33.06A86.93,86.93,0,0,0,162,203.18C205,177.18,227.93,116.21,223.45,40.07ZM153.75,189.5c-22.75,13.78-49.68,14-76.71.77l88.63-88.62a8,8,0,0,0-11.32-11.32L65.73,179c-13.19-27-13-54,.77-76.71,22.09-36.47,74.6-56.44,141.31-54.06C210.2,114.89,190.22,167.41,153.75,189.5Z"/></g>"#;

/// A car's wheels and axles, seen from above.
/// Phosphor's `steering-wheel`.
const AXLES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M222.4,145.55A96.6,96.6,0,0,1,218.54,160h-53a8,8,0,0,0-7.5,5.19l-21.9,58.47c-2.69.22-5.41.34-8.15.34s-5.5-.12-8.2-.35L97.94,165.2a8,8,0,0,0-7.49-5.2h-53a96.6,96.6,0,0,1-3.86-14.45,128,128,0,0,1,188.8,0Z"/><path fill="currentColor" d="M128,152a12,12,0,1,1,12-12A12,12,0,0,1,128,152Zm104-24A104,104,0,1,1,128,24,104.11,104.11,0,0,1,232,128ZM40,128v.33a135.93,135.93,0,0,1,176,0V128a88,88,0,0,0-176,0Zm67.5,85.58L90.45,168H49.63A88.35,88.35,0,0,0,107.5,213.58ZM128,216c.83,0,1.66,0,2.49,0l20.07-53.57a16.07,16.07,0,0,1,15-10.39h47.12c.38-1.31.72-2.64,1-4a120,120,0,0,0-171.4,0c.31,1.34.65,2.67,1,4H90.45a16.08,16.08,0,0,1,15,10.4l20,53.56C126.31,216,127.15,216,128,216Zm78.37-48H165.55l-17.09,45.59A88.34,88.34,0,0,0,206.37,168Z"/></g>"#;

/// A car's door, its window.
/// Phosphor's `door`.
const CAR_DOOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,40V224H56V40a8,8,0,0,1,8-8H192A8,8,0,0,1,200,40Z"/><path fill="currentColor" d="M232,216H208V40a16,16,0,0,0-16-16H64A16,16,0,0,0,48,40V216H24a8,8,0,0,0,0,16H232a8,8,0,0,0,0-16ZM64,40H192V216H64Zm104,92a12,12,0,1,1-12-12A12,12,0,0,1,168,132Z"/></g>"#;

/// A car seat.
/// Phosphor's `seat`.
const SEAT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,160v32a8,8,0,0,1-8,8H114.11a8,8,0,0,1-7.15-4.42l-58.12-116a8,8,0,0,1,0-7.16L71,28.42a8,8,0,0,1,10.73-3.57l33.89,14.31a8,8,0,0,1,3.57,10.73L104.84,76.42a8,8,0,0,0,0,7.16l31.78,64a8,8,0,0,0,7.16,4.42H208A8,8,0,0,1,216,160Z"/><path fill="currentColor" d="M224,232a8,8,0,0,1-8,8H112a8,8,0,0,1,0-16H216A8,8,0,0,1,224,232Zm0-72v32a16,16,0,0,1-16,16H114.11a15.93,15.93,0,0,1-14.32-8.85l-58.11-116a16.1,16.1,0,0,1,0-14.32l22.12-44A16,16,0,0,1,85,17.56l33.69,14.22.47.22a16,16,0,0,1,7.15,21.46,1.51,1.51,0,0,1-.11.22L112,80l31.78,64L208,144A16,16,0,0,1,224,160Zm-16,0H143.77a15.91,15.91,0,0,1-14.31-8.85l-31.79-64a16.07,16.07,0,0,1,0-14.29l.12-.22L112,46.32,78.57,32.21c-.16-.06-.31-.14-.46-.21L56,76,114.1,192H208Z"/></g>"#;

/// Two people.
/// Phosphor's `users`.
const PEOPLE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M136,108A52,52,0,1,1,84,56,52,52,0,0,1,136,108Z"/><path fill="currentColor" d="M117.25,157.92a60,60,0,1,0-66.5,0A95.83,95.83,0,0,0,3.53,195.63a8,8,0,1,0,13.4,8.74,80,80,0,0,1,134.14,0,8,8,0,0,0,13.4-8.74A95.83,95.83,0,0,0,117.25,157.92ZM40,108a44,44,0,1,1,44,44A44.05,44.05,0,0,1,40,108Zm210.14,98.7a8,8,0,0,1-11.07-2.33A79.83,79.83,0,0,0,172,168a8,8,0,0,1,0-16,44,44,0,1,0-16.34-84.87,8,8,0,1,1-5.94-14.85,60,60,0,0,1,55.53,105.64,95.83,95.83,0,0,1,47.22,37.71A8,8,0,0,1,250.14,206.7Z"/></g>"#;

/// A shield with a check.
/// Phosphor's `shield-check`.
const SHIELD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,56v56c0,96-88,120-88,120S40,208,40,112V56a8,8,0,0,1,8-8H208A8,8,0,0,1,216,56Z"/><path fill="currentColor" d="M208,40H48A16,16,0,0,0,32,56v56c0,52.72,25.52,84.67,46.93,102.19,23.06,18.86,46,25.26,47,25.53a8,8,0,0,0,4.2,0c1-.27,23.91-6.67,47-25.53C198.48,196.67,224,164.72,224,112V56A16,16,0,0,0,208,40Zm0,72c0,37.07-13.66,67.16-40.6,89.42A129.3,129.3,0,0,1,128,223.62a128.25,128.25,0,0,1-38.92-21.81C61.82,179.51,48,149.3,48,112l0-56,160,0ZM82.34,141.66a8,8,0,0,1,11.32-11.32L112,148.69l50.34-50.35a8,8,0,0,1,11.32,11.32l-56,56a8,8,0,0,1-11.32,0Z"/></g>"#;

/// A closed book.
/// Phosphor's `book`.
const BOOK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,32V192H72a24,24,0,0,0-24,24V56A24,24,0,0,1,72,32Z"/><path fill="currentColor" d="M208,24H72A32,32,0,0,0,40,56V224a8,8,0,0,0,8,8H192a8,8,0,0,0,0-16H56a16,16,0,0,1,16-16H208a8,8,0,0,0,8-8V32A8,8,0,0,0,208,24Zm-8,160H72a31.82,31.82,0,0,0-16,4.29V56A16,16,0,0,1,72,40H200Z"/></g>"#;

/// A clipboard with a check.
/// Phosphor's `clipboard-text`.
const CLIPBOARD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,48V216a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8H96a39.83,39.83,0,0,0-8,24v8h80V64a39.83,39.83,0,0,0-8-24h40A8,8,0,0,1,208,48Z"/><path fill="currentColor" d="M168,152a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,152Zm-8-40H96a8,8,0,0,0,0,16h64a8,8,0,0,0,0-16Zm56-64V216a16,16,0,0,1-16,16H56a16,16,0,0,1-16-16V48A16,16,0,0,1,56,32H92.26a47.92,47.92,0,0,1,71.48,0H200A16,16,0,0,1,216,48ZM96,64h64a32,32,0,0,0-64,0ZM200,48H173.25A47.93,47.93,0,0,1,176,64v8a8,8,0,0,1-8,8H88a8,8,0,0,1-8-8V64a47.93,47.93,0,0,1,2.75-16H56V216H200Z"/></g>"#;

/// An umbrella.
/// Phosphor's `umbrella`.
const UMBRELLA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,32S88,64,88,136H32a8,8,0,0,1-8-8.71A104.21,104.21,0,0,1,128,32Zm104,95.29A104.21,104.21,0,0,0,128,32s40,32,40,104h56A8,8,0,0,0,232,127.29Z"/><path fill="currentColor" d="M240,126.63A112.44,112.44,0,0,0,51.75,53.75a111.56,111.56,0,0,0-35.7,72.88A16,16,0,0,0,32,144h88v56a32,32,0,0,0,64,0,8,8,0,0,0-16,0,16,16,0,0,1-32,0V144h88a16,16,0,0,0,16-17.37ZM32,128l0,0a96.15,96.15,0,0,1,76.2-85.89C96.48,58,81.85,86.11,80.17,128Zm64.15,0c1.39-30.77,10.53-52.81,18.3-66.24A106.44,106.44,0,0,1,128,43.16a106.31,106.31,0,0,1,13.52,18.6C154.8,84.7,159,109.28,159.82,128Zm79.65,0c-1.68-41.89-16.31-70-28-85.94A96.07,96.07,0,0,1,224,128Z"/></g>"#;

/// A motorcyclist's helmet, its visor.
/// Phosphor's `hard-hat`.
const HELMET: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M104,51.31V160H40V136A88,88,0,0,1,104,51.31Zm48,0V160h64V136A88,88,0,0,0,152,51.31Z"/><path fill="currentColor" d="M224,152V136a96.37,96.37,0,0,0-64-90.51V40a16,16,0,0,0-16-16H112A16,16,0,0,0,96,40v5.49A96.37,96.37,0,0,0,32,136v16a16,16,0,0,0-16,16v24a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V168A16,16,0,0,0,224,152Zm-16-16v16H160V62.67A80.36,80.36,0,0,1,208,136ZM144,40V152H112V40ZM48,136A80.36,80.36,0,0,1,96,62.67V152H48Zm176,56H32V168H224v24Z"/></g>"#;

/// A licence card.
/// Phosphor's `identification-card`.
const ID_CARD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,48H40a8,8,0,0,0-8,8V200a8,8,0,0,0,8,8H216a8,8,0,0,0,8-8V56A8,8,0,0,0,216,48ZM96,144a24,24,0,1,1,24-24A24,24,0,0,1,96,144Z"/><path fill="currentColor" d="M200,112a8,8,0,0,1-8,8H152a8,8,0,0,1,0-16h40A8,8,0,0,1,200,112Zm-8,24H152a8,8,0,0,0,0,16h40a8,8,0,0,0,0-16Zm40-80V200a16,16,0,0,1-16,16H40a16,16,0,0,1-16-16V56A16,16,0,0,1,40,40H216A16,16,0,0,1,232,56ZM216,200V56H40V200H216Zm-80.26-34a8,8,0,1,1-15.5,4c-2.63-10.26-13.06-18-24.25-18s-21.61,7.74-24.25,18a8,8,0,1,1-15.5-4,39.84,39.84,0,0,1,17.19-23.34,32,32,0,1,1,45.12,0A39.76,39.76,0,0,1,135.75,166ZM96,136a16,16,0,1,0-16-16A16,16,0,0,0,96,136Z"/></g>"#;

/// A van.
/// Phosphor's `van`.
const VAN: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,112v64a8,8,0,0,1-8,8H216a24,24,0,0,0-48,0H104a24,24,0,0,0-48,0H32a8,8,0,0,1-8-8V112Z"/><path fill="currentColor" d="M254.07,106.79,208.53,53.73A16,16,0,0,0,196.26,48H32A16,16,0,0,0,16,64V176a16,16,0,0,0,16,16H49a32,32,0,0,0,62,0h50a32,32,0,0,0,62,0h17a16,16,0,0,0,16-16V112A8,8,0,0,0,254.07,106.79ZM230.59,104H176V64h20.26ZM104,104V64h56v40ZM88,64v40H32V64ZM80,200a16,16,0,1,1,16-16A16,16,0,0,1,80,200Zm112,0a16,16,0,1,1,16-16A16,16,0,0,1,192,200Zm31-24a32,32,0,0,0-62,0H111a32,32,0,0,0-62,0H32V120H240v56Z"/></g>"#;

/// A kettlebell.
/// Phosphor's `scales`.
const WEIGHT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M56,88l32,80c0,17.67-20,24-32,24s-32-6.33-32-24ZM200,56l-32,80c0,17.67,20,24,32,24s32-6.33,32-24Z"/><path fill="currentColor" d="M239.43,133l-32-80h0a8,8,0,0,0-9.16-4.84L136,62V40a8,8,0,0,0-16,0V65.58L54.26,80.19A8,8,0,0,0,48.57,85h0v.06L16.57,165a7.92,7.92,0,0,0-.57,3c0,23.31,24.54,32,40,32s40-8.69,40-32a7.92,7.92,0,0,0-.57-3L66.92,93.77,120,82V208H104a8,8,0,0,0,0,16h48a8,8,0,0,0,0-16H136V78.42L187,67.1,160.57,133a7.92,7.92,0,0,0-.57,3c0,23.31,24.54,32,40,32s40-8.69,40-32A7.92,7.92,0,0,0,239.43,133ZM56,184c-7.53,0-22.76-3.61-23.93-14.64L56,109.54l23.93,59.82C78.76,180.39,63.53,184,56,184Zm144-32c-7.53,0-22.76-3.61-23.93-14.64L200,77.54l23.93,59.82C222.76,148.39,207.53,152,200,152Z"/></g>"#;

/// A number sign.
/// Phosphor's `hash`.
const HASH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M165.82,96l-11.64,64h-64l11.64-64Z"/><path fill="currentColor" d="M224,88H175.4l8.47-46.57a8,8,0,0,0-15.74-2.86l-9,49.43H111.4l8.47-46.57a8,8,0,0,0-15.74-2.86L95.14,88H48a8,8,0,0,0,0,16H92.23L83.5,152H32a8,8,0,0,0,0,16H80.6l-8.47,46.57a8,8,0,0,0,6.44,9.3A7.79,7.79,0,0,0,80,224a8,8,0,0,0,7.86-6.57l9-49.43H144.6l-8.47,46.57a8,8,0,0,0,6.44,9.3A7.79,7.79,0,0,0,144,224a8,8,0,0,0,7.86-6.57l9-49.43H208a8,8,0,0,0,0-16H163.77l8.73-48H224a8,8,0,0,0,0-16Zm-76.5,64H99.77l8.73-48h47.73Z"/></g>"#;

/// A circle's width, across it.
/// Phosphor's `arrows-out-line-horizontal`.
const DIAMETER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,56V200a16,16,0,0,1-16,16H32a16,16,0,0,1-16-16V56A16,16,0,0,1,32,40H224A16,16,0,0,1,240,56Z"/><path fill="currentColor" d="M136,40V216a8,8,0,0,1-16,0V40a8,8,0,0,1,16,0ZM96,120H35.31l18.35-18.34A8,8,0,0,0,42.34,90.34l-32,32a8,8,0,0,0,0,11.32l32,32a8,8,0,0,0,11.32-11.32L35.31,136H96a8,8,0,0,0,0-16Zm149.66,2.34-32-32a8,8,0,0,0-11.32,11.32L220.69,120H160a8,8,0,0,0,0,16h60.69l-18.35,18.34a8,8,0,0,0,11.32,11.32l32-32A8,8,0,0,0,245.66,122.34Z"/></g>"#;

/// A width between its arrows.
/// Phosphor's `arrows-horizontal`.
const WIDTH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,128l-32,32H56L24,128,56,96H200Z"/><path fill="currentColor" d="M237.66,133.66l-32,32a8,8,0,0,1-11.32-11.32L212.69,136H43.31l18.35,18.34a8,8,0,0,1-11.32,11.32l-32-32a8,8,0,0,1,0-11.32l32-32a8,8,0,0,1,11.32,11.32L43.31,120H212.69l-18.35-18.34a8,8,0,0,1,11.32-11.32l32,32A8,8,0,0,1,237.66,133.66Z"/></g>"#;

/// A height between its arrows.
/// Phosphor's `arrows-vertical`.
const HEIGHT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M160,56V200l-32,32L96,200V56l32-32Z"/><path fill="currentColor" d="M165.66,194.34a8,8,0,0,1,0,11.32l-32,32a8,8,0,0,1-11.32,0l-32-32a8,8,0,0,1,11.32-11.32L120,212.69V43.31L101.66,61.66A8,8,0,0,1,90.34,50.34l32-32a8,8,0,0,1,11.32,0l32,32a8,8,0,0,1-11.32,11.32L136,43.31V212.69l18.34-18.35A8,8,0,0,1,165.66,194.34Z"/></g>"#;

/// A thermometer.
/// Phosphor's `thermometer`.
const THERMOMETER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M152,138V48a32,32,0,0,0-64,0v90a56,56,0,1,0,64,0Zm-32,70a24,24,0,1,1,24-24A24,24,0,0,1,120,208Z"/><path fill="currentColor" d="M212,56a28,28,0,1,0,28,28A28,28,0,0,0,212,56Zm0,40a12,12,0,1,1,12-12A12,12,0,0,1,212,96Zm-84,57V88a8,8,0,0,0-16,0v65a32,32,0,1,0,16,0Zm-8,47a16,16,0,1,1,16-16A16,16,0,0,1,120,200Zm40-66V48a40,40,0,0,0-80,0v86a64,64,0,1,0,80,0Zm-40,98a48,48,0,0,1-27.42-87.4A8,8,0,0,0,96,138V48a24,24,0,0,1,48,0v90a8,8,0,0,0,3.42,6.56A48,48,0,0,1,120,232Z"/></g>"#;

/// Layers.
/// Phosphor's `stack`.
const LAYERS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,80l-96,56L32,80l96-56Z"/><path fill="currentColor" d="M230.91,172A8,8,0,0,1,228,182.91l-96,56a8,8,0,0,1-8.06,0l-96-56A8,8,0,0,1,36,169.09l92,53.65,92-53.65A8,8,0,0,1,230.91,172ZM220,121.09l-92,53.65L36,121.09A8,8,0,0,0,28,134.91l96,56a8,8,0,0,0,8.06,0l96-56A8,8,0,1,0,220,121.09ZM24,80a8,8,0,0,1,4-6.91l96-56a8,8,0,0,1,8.06,0l96,56a8,8,0,0,1,0,13.82l-96,56a8,8,0,0,1-8.06,0l-96-56A8,8,0,0,1,24,80Zm23.88,0L128,126.74,208.12,80,128,33.26Z"/></g>"#;

/// A plug.
/// Phosphor's `plug`.
const PLUG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M212,132l-58.63,58.63a32,32,0,0,1-45.25,0L65.37,147.88a32,32,0,0,1,0-45.25L124,44Z"/><path fill="currentColor" d="M237.66,66.34a8,8,0,0,0-11.32,0L192,100.69,155.31,64l34.35-34.34a8,8,0,1,0-11.32-11.32L144,52.69,117.66,26.34a8,8,0,0,0-11.32,11.32L112.69,44l-53,53a40,40,0,0,0,0,56.57l15.71,15.71L26.34,218.34a8,8,0,0,0,11.32,11.32l49.09-49.09,15.71,15.71a40,40,0,0,0,56.57,0l53-53,6.34,6.35a8,8,0,0,0,11.32-11.32L203.31,112l34.35-34.34A8,8,0,0,0,237.66,66.34ZM147.72,185a24,24,0,0,1-33.95,0L71,142.23a24,24,0,0,1,0-33.95l53-53L200.69,132Z"/></g>"#;

/// A floor's size, arrows across it.
/// Phosphor's `arrows-out`.
const AREA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,48V208H48V48Z"/><path fill="currentColor" d="M216,48V96a8,8,0,0,1-16,0V67.31l-42.34,42.35a8,8,0,0,1-11.32-11.32L188.69,56H160a8,8,0,0,1,0-16h48A8,8,0,0,1,216,48ZM98.34,146.34,56,188.69V160a8,8,0,0,0-16,0v48a8,8,0,0,0,8,8H96a8,8,0,0,0,0-16H67.31l42.35-42.34a8,8,0,0,0-11.32-11.32ZM208,152a8,8,0,0,0-8,8v28.69l-42.34-42.35a8,8,0,0,0-11.32,11.32L188.69,200H160a8,8,0,0,0,0,16h48a8,8,0,0,0,8-8V160A8,8,0,0,0,208,152ZM67.31,56H96a8,8,0,0,0,0-16H48a8,8,0,0,0-8,8V96a8,8,0,0,0,16,0V67.31l42.34,42.35a8,8,0,0,0,11.32-11.32Z"/></g>"#;

/// A bathtub.
/// Phosphor's `bathtub`.
const BATH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,104v40a48,48,0,0,1-48,48H64a48,48,0,0,1-48-48V104H136v40h64V104Z"/><path fill="currentColor" d="M240,96H208a8,8,0,0,0-8-8H136a8,8,0,0,0-8,8H64V52A12,12,0,0,1,76,40a12.44,12.44,0,0,1,12.16,9.59,8,8,0,0,0,15.68-3.18A28.32,28.32,0,0,0,76,24,28,28,0,0,0,48,52V96H16a8,8,0,0,0-8,8v40a56.06,56.06,0,0,0,56,56v16a8,8,0,0,0,16,0V200h96v16a8,8,0,0,0,16,0V200a56.06,56.06,0,0,0,56-56V104A8,8,0,0,0,240,96Zm-48,8v32H144V104Zm40,40a40,40,0,0,1-40,40H64a40,40,0,0,1-40-40V112H128v32a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V112h24Z"/></g>"#;

/// Stairs.
/// Phosphor's `steps`.
const STAIRS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,56V184a16,16,0,0,1-16,16H72V152h56V104h56V56Z"/><path fill="currentColor" d="M248,56a8,8,0,0,1-8,8H192v40a8,8,0,0,1-8,8H136v40a8,8,0,0,1-8,8H80v40a8,8,0,0,1-8,8H16a8,8,0,0,1,0-16H64V152a8,8,0,0,1,8-8h48V104a8,8,0,0,1,8-8h48V56a8,8,0,0,1,8-8h56A8,8,0,0,1,248,56Z"/></g>"#;

/// A block of flats, floor over floor.
/// Phosphor's `building-apartment`.
const BUILDING: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,72V216H144V168H112v48H32V104H80V40h96V72Z"/><path fill="currentColor" d="M240,208h-8V72a8,8,0,0,0-8-8H184V40a8,8,0,0,0-8-8H80a8,8,0,0,0-8,8V96H32a8,8,0,0,0-8,8V208H16a8,8,0,0,0,0,16H240a8,8,0,0,0,0-16ZM40,112H80a8,8,0,0,0,8-8V48h80V72a8,8,0,0,0,8,8h40V208H152V168a8,8,0,0,0-8-8H112a8,8,0,0,0-8,8v40H40Zm96,96H120V176h16ZM112,72a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H120A8,8,0,0,1,112,72Zm0,32a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H120A8,8,0,0,1,112,104Zm56,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H176A8,8,0,0,1,168,104ZM88,136a8,8,0,0,1-8,8H64a8,8,0,0,1,0-16H80A8,8,0,0,1,88,136Zm0,32a8,8,0,0,1-8,8H64a8,8,0,0,1,0-16H80A8,8,0,0,1,88,168Zm24-32a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H120A8,8,0,0,1,112,136Zm56,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H176A8,8,0,0,1,168,136Zm0,32a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H176A8,8,0,0,1,168,168Z"/></g>"#;

/// A sparkle.
/// Phosphor's `sparkle`.
pub(super) const SPARKLES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M194.82,151.43l-55.09,20.3-20.3,55.09a7.92,7.92,0,0,1-14.86,0l-20.3-55.09-55.09-20.3a7.92,7.92,0,0,1,0-14.86l55.09-20.3,20.3-55.09a7.92,7.92,0,0,1,14.86,0l20.3,55.09,55.09,20.3A7.92,7.92,0,0,1,194.82,151.43Z"/><path fill="currentColor" d="M197.58,129.06,146,110l-19-51.62a15.92,15.92,0,0,0-29.88,0L78,110l-51.62,19a15.92,15.92,0,0,0,0,29.88L78,178l19,51.62a15.92,15.92,0,0,0,29.88,0L146,178l51.62-19a15.92,15.92,0,0,0,0-29.88ZM137,164.22a8,8,0,0,0-4.74,4.74L112,223.85,91.78,169A8,8,0,0,0,87,164.22L32.15,144,87,123.78A8,8,0,0,0,91.78,119L112,64.15,132.22,119a8,8,0,0,0,4.74,4.74L191.85,144ZM144,40a8,8,0,0,1,8-8h16V16a8,8,0,0,1,16,0V32h16a8,8,0,0,1,0,16H184V64a8,8,0,0,1-16,0V48H152A8,8,0,0,1,144,40ZM248,88a8,8,0,0,1-8,8h-8v8a8,8,0,0,1-16,0V96h-8a8,8,0,0,1,0-16h8V72a8,8,0,0,1,16,0v8h8A8,8,0,0,1,248,88Z"/></g>"#;

/// A radiator.
/// Phosphor's `thermometer-hot`.
const RADIATOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M152,138V48a32,32,0,0,0-64,0v90a56,56,0,1,0,64,0Zm-32,70a24,24,0,1,1,24-24A24,24,0,0,1,120,208Z"/><path fill="currentColor" d="M128,153V48a8,8,0,0,0-16,0V153a32,32,0,1,0,16,0Zm-8,47a16,16,0,1,1,16-16A16,16,0,0,1,120,200Zm40-66V48a40,40,0,0,0-80,0v86a64,64,0,1,0,80,0Zm-40,98a48,48,0,0,1-27.42-87.4A8,8,0,0,0,96,138V48a24,24,0,0,1,48,0v90a8,8,0,0,0,3.42,6.56A48,48,0,0,1,120,232ZM177.3,80.77a8,8,0,0,1,2.33-11.07c15-9.79,26.87-4.75,35.51-1.06C223,72,227.76,74,235.63,68.89a8,8,0,0,1,8.74,13.41C237.88,86.53,232,88,226.69,88c-7,0-12.92-2.54-17.83-4.63C201,80,196.24,78,188.37,83.11A8,8,0,0,1,177.3,80.77Zm69.4,22.46a8,8,0,0,1-2.33,11.07C237.88,118.53,232,120,226.69,120c-7,0-12.92-2.54-17.83-4.63-7.87-3.36-12.62-5.38-20.49-.25a8,8,0,0,1-8.74-13.41c15-9.79,26.87-4.75,35.51-1.06,7.87,3.36,12.62,5.39,20.49.25A8,8,0,0,1,246.7,103.23Z"/></g>"#;

/// A fence.
/// Phosphor's `barricade`.
const FENCE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,80v68L156,72h68A8,8,0,0,1,232,80ZM32,72a8,8,0,0,0-8,8v4l76,76h72L84,72Z"/><path fill="currentColor" d="M224,64H32A16,16,0,0,0,16,80v72a16,16,0,0,0,16,16H56v32a8,8,0,0,0,16,0V168H184v32a8,8,0,0,0,16,0V168h24a16,16,0,0,0,16-16V80A16,16,0,0,0,224,64Zm0,64.69L175.31,80H224ZM80.69,80l72,72H103.31L32,80.69V80ZM32,103.31,80.69,152H32ZM224,152H175.31l-72-72h49.38L224,151.32V152Z"/></g>"#;

/// Hills under the sun.
/// Phosphor's `mountains`.
const HILLS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M144,52a20,20,0,1,1,20,20A20,20,0,0,1,144,52Zm49.44,55.92a8,8,0,0,0-13.77,0l-33,55.75-21-35.67H50.35L8,200H248Z"/><path fill="currentColor" d="M164,80a28,28,0,1,0-28-28A28,28,0,0,0,164,80Zm0-40a12,12,0,1,1-12,12A12,12,0,0,1,164,40Zm90.88,155.92-54.56-92.08A15.87,15.87,0,0,0,186.55,96h0a15.85,15.85,0,0,0-13.76,7.84L146.63,148l-44.84-76.1a16,16,0,0,0-27.58,0L1.11,195.94A8,8,0,0,0,8,208H248a8,8,0,0,0,6.88-12.08ZM88,80l23.57,40H64.43ZM22,192l33-56h66l33,56Zm150.57,0-16.66-28.28L186.55,112,234,192Z"/></g>"#;

/// A document with a check.
/// Phosphor's `certificate`.
const PERMIT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V90.06h0a44,44,0,1,0-56,67.88h0V192H40a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z"/><path fill="currentColor" d="M128,136a8,8,0,0,1-8,8H72a8,8,0,0,1,0-16h48A8,8,0,0,1,128,136Zm-8-40H72a8,8,0,0,0,0,16h48a8,8,0,0,0,0-16Zm112,65.47V224A8,8,0,0,1,220,231l-24-13.74L172,231A8,8,0,0,1,160,224V200H40a16,16,0,0,1-16-16V56A16,16,0,0,1,40,40H216a16,16,0,0,1,16,16V86.53a51.88,51.88,0,0,1,0,74.94ZM160,184V161.47A52,52,0,0,1,216,76V56H40V184Zm56-12a51.88,51.88,0,0,1-40,0v38.22l16-9.16a8,8,0,0,1,7.94,0l16,9.16Zm16-48a36,36,0,1,0-36,36A36,36,0,0,0,232,124Z"/></g>"#;

/// A road.
/// Phosphor's `road-horizon`.
const ROAD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,192H24L96,64h64Z"/><path fill="currentColor" d="M235.92,199A8,8,0,0,1,225,195.92L155.32,72H136v8a8,8,0,0,1-16,0V72H100.68L31,195.92A8,8,0,0,1,17,188.08L82.32,72H24a8,8,0,0,1,0-16H232a8,8,0,0,1,0,16H173.68L239,188.08A8,8,0,0,1,235.92,199ZM128,112a8,8,0,0,0-8,8v16a8,8,0,0,0,16,0V120A8,8,0,0,0,128,112Zm0,56a8,8,0,0,0-8,8v16a8,8,0,0,0,16,0V176A8,8,0,0,0,128,168Z"/></g>"#;

/// A parking sign.
/// Phosphor's `letter-circle-p`.
const PARKING: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm8-136H104a8,8,0,0,0-8,8v80a8,8,0,0,0,16,0V152h24a36,36,0,0,0,0-72Zm0,56H112V96h24a20,20,0,0,1,0,40Z"/></g>"#;

/// A banknote.
/// Phosphor's `money`.
const BANKNOTE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M160,128a32,32,0,1,1-32-32A32,32,0,0,1,160,128Zm40-64a48.85,48.85,0,0,0,40,40V64Zm0,128h40V152A48.85,48.85,0,0,0,200,192ZM16,152v40H56A48.85,48.85,0,0,0,16,152Zm0-48A48.85,48.85,0,0,0,56,64H16Z"/><path fill="currentColor" d="M128,88a40,40,0,1,0,40,40A40,40,0,0,0,128,88Zm0,64a24,24,0,1,1,24-24A24,24,0,0,1,128,152ZM240,56H16a8,8,0,0,0-8,8V192a8,8,0,0,0,8,8H240a8,8,0,0,0,8-8V64A8,8,0,0,0,240,56ZM24,72H45.37A40.81,40.81,0,0,1,24,93.37Zm0,112V162.63A40.81,40.81,0,0,1,45.37,184Zm208,0H210.63A40.81,40.81,0,0,1,232,162.63Zm0-38.35A56.78,56.78,0,0,0,193.65,184H62.35A56.78,56.78,0,0,0,24,145.65v-35.3A56.78,56.78,0,0,0,62.35,72h131.3A56.78,56.78,0,0,0,232,110.35Zm0-52.28A40.81,40.81,0,0,1,210.63,72H232Z"/></g>"#;

/// A light bulb.
/// Phosphor's `lightbulb`.
const LIGHT_BULB: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,104a79.86,79.86,0,0,1-30.59,62.92A24.29,24.29,0,0,0,168,186v6a8,8,0,0,1-8,8H96a8,8,0,0,1-8-8v-6a24.11,24.11,0,0,0-9.3-19A79.87,79.87,0,0,1,48,104.45C47.76,61.09,82.72,25,126.07,24A80,80,0,0,1,208,104Z"/><path fill="currentColor" d="M176,232a8,8,0,0,1-8,8H88a8,8,0,0,1,0-16h80A8,8,0,0,1,176,232Zm40-128a87.55,87.55,0,0,1-33.64,69.21A16.24,16.24,0,0,0,176,186v6a16,16,0,0,1-16,16H96a16,16,0,0,1-16-16v-6a16,16,0,0,0-6.23-12.66A87.59,87.59,0,0,1,40,104.49C39.74,56.83,78.26,17.14,125.88,16A88,88,0,0,1,216,104Zm-16,0a72,72,0,0,0-73.74-72c-39,.92-70.47,33.39-70.26,72.39a71.65,71.65,0,0,0,27.64,56.3A32,32,0,0,1,96,186v6h64v-6a32.15,32.15,0,0,1,12.47-25.35A71.65,71.65,0,0,0,200,104Zm-16.11-9.34a57.6,57.6,0,0,0-46.56-46.55,8,8,0,0,0-2.66,15.78c16.57,2.79,30.63,16.85,33.44,33.45A8,8,0,0,0,176,104a9,9,0,0,0,1.35-.11A8,8,0,0,0,183.89,94.66Z"/></g>"#;

/// A graduate's cap.
/// Phosphor's `graduation-cap`.
const MORTARBOARD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,113.07v53.22a8,8,0,0,1-2,5.31c-11.3,12.59-38.9,36.4-86,36.4s-74.68-23.81-86-36.4a8,8,0,0,1-2-5.31V113.07L128,160Z"/><path fill="currentColor" d="M251.76,88.94l-120-64a8,8,0,0,0-7.52,0l-120,64a8,8,0,0,0,0,14.12L32,117.87v48.42a15.91,15.91,0,0,0,4.06,10.65C49.16,191.53,78.51,216,128,216a130,130,0,0,0,48-8.76V240a8,8,0,0,0,16,0V199.51a115.63,115.63,0,0,0,27.94-22.57A15.91,15.91,0,0,0,224,166.29V117.87l27.76-14.81a8,8,0,0,0,0-14.12ZM128,200c-43.27,0-68.72-21.14-80-33.71V126.4l76.24,40.66a8,8,0,0,0,7.52,0L176,143.47v46.34C163.4,195.69,147.52,200,128,200Zm80-33.75a97.83,97.83,0,0,1-16,14.25V134.93l16-8.53ZM188,118.94l-.22-.13-56-29.87a8,8,0,0,0-7.52,14.12L171,128l-43,22.93L25,96,128,41.07,231,96Z"/></g>"#;

/// A memory card.
/// Phosphor's `sim-card`.
const MEMORY_CARD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,120v72H80V120Z"/><path fill="currentColor" d="M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM200,216H56V40h92.69L200,91.31V216ZM176,112H80a8,8,0,0,0-8,8v72a8,8,0,0,0,8,8h96a8,8,0,0,0,8-8V120A8,8,0,0,0,176,112Zm-8,72H152V152a8,8,0,0,0-16,0v32H120V152a8,8,0,0,0-16,0v32H88V128h80Z"/></g>"#;

/// A memory module.
/// Phosphor's `memory`.
const RAM: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,64H24a8,8,0,0,0-8,8V176H240V72A8,8,0,0,0,232,64ZM112,144H48V96h64Zm96,0H144V96h64Z"/><path fill="currentColor" d="M232,56H24A16,16,0,0,0,8,72V200a8,8,0,0,0,16,0V184H40v16a8,8,0,0,0,16,0V184H72v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V72A16,16,0,0,0,232,56ZM24,72H232v96H24Zm88,80a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H48a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8ZM56,104h48v32H56Zm88,48h64a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H144a8,8,0,0,0-8,8v48A8,8,0,0,0,144,152Zm8-48h48v32H152Z"/></g>"#;

/// A battery.
/// Phosphor's `battery-high`.
const BATTERY: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,80v96a16,16,0,0,1-16,16H32a16,16,0,0,1-16-16V80A16,16,0,0,1,32,64H200A16,16,0,0,1,216,80Z"/><path fill="currentColor" d="M200,56H32A24,24,0,0,0,8,80v96a24,24,0,0,0,24,24H200a24,24,0,0,0,24-24V80A24,24,0,0,0,200,56Zm8,120a8,8,0,0,1-8,8H32a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8H200a8,8,0,0,1,8,8ZM144,96v64a8,8,0,0,1-16,0V96a8,8,0,0,1,16,0Zm-40,0v64a8,8,0,0,1-16,0V96a8,8,0,0,1,16,0ZM64,96v64a8,8,0,0,1-16,0V96a8,8,0,0,1,16,0Zm192,0v64a8,8,0,0,1-16,0V96a8,8,0,0,1,16,0Z"/></g>"#;

/// A chip.
/// Phosphor's `cpu`.
const CHIP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,48H56a8,8,0,0,0-8,8V200a8,8,0,0,0,8,8H200a8,8,0,0,0,8-8V56A8,8,0,0,0,200,48ZM152,152H104V104h48Z"/><path fill="currentColor" d="M152,96H104a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V104A8,8,0,0,0,152,96Zm-8,48H112V112h32Zm88,0H216V112h16a8,8,0,0,0,0-16H216V56a16,16,0,0,0-16-16H160V24a8,8,0,0,0-16,0V40H112V24a8,8,0,0,0-16,0V40H56A16,16,0,0,0,40,56V96H24a8,8,0,0,0,0,16H40v32H24a8,8,0,0,0,0,16H40v40a16,16,0,0,0,16,16H96v16a8,8,0,0,0,16,0V216h32v16a8,8,0,0,0,16,0V216h40a16,16,0,0,0,16-16V160h16a8,8,0,0,0,0-16Zm-32,56H56V56H200v95.87s0,.09,0,.13,0,.09,0,.13V200Z"/></g>"#;

/// A drive.
/// Phosphor's `hard-drive`.
const DRIVE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,80v96a8,8,0,0,1-8,8H32a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8H224A8,8,0,0,1,232,80Z"/><path fill="currentColor" d="M224,64H32A16,16,0,0,0,16,80v96a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V80A16,16,0,0,0,224,64Zm0,112H32V80H224v96Zm-24-48a12,12,0,1,1-12-12A12,12,0,0,1,200,128Z"/></g>"#;

/// A graphics card, its fans.
/// Phosphor's `graphics-card`.
const GRAPHICS_CARD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,56H16V184H232a8,8,0,0,0,8-8V64A8,8,0,0,0,232,56ZM80,152a32,32,0,1,1,32-32A32,32,0,0,1,80,152Zm96,0a32,32,0,1,1,32-32A32,32,0,0,1,176,152Z"/><path fill="currentColor" d="M232,48H16a8,8,0,0,0-8,8V208a8,8,0,0,0,16,0V192H40v16a8,8,0,0,0,16,0V192H72v16a8,8,0,0,0,16,0V192h16v16a8,8,0,0,0,16,0V192H232a16,16,0,0,0,16-16V64A16,16,0,0,0,232,48Zm0,128H24V64H232Zm-56-16a40,40,0,1,0-40-40A40,40,0,0,0,176,160Zm-24-40a23.74,23.74,0,0,1,2.35-10.34l32,32A23.74,23.74,0,0,1,176,144,24,24,0,0,1,152,120Zm48,0a23.74,23.74,0,0,1-2.35,10.34l-32-32A23.74,23.74,0,0,1,176,96,24,24,0,0,1,200,120ZM80,160a40,40,0,1,0-40-40A40,40,0,0,0,80,160ZM56,120a23.74,23.74,0,0,1,2.35-10.34l32,32A23.74,23.74,0,0,1,80,144,24,24,0,0,1,56,120Zm48,0a23.74,23.74,0,0,1-2.35,10.34l-32-32A23.74,23.74,0,0,1,80,96,24,24,0,0,1,104,120Z"/></g>"#;

/// Four windows.
/// Phosphor's `squares-four`.
const APPS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M112,56v48a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8h48A8,8,0,0,1,112,56Zm88-8H152a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V56A8,8,0,0,0,200,48Zm-96,96H56a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V152A8,8,0,0,0,104,144Zm96,0H152a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V152A8,8,0,0,0,200,144Z"/><path fill="currentColor" d="M200,136H152a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V152A16,16,0,0,0,200,136Zm0,64H152V152h48v48ZM104,40H56A16,16,0,0,0,40,56v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V56A16,16,0,0,0,104,40Zm0,64H56V56h48v48Zm96-64H152a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V56A16,16,0,0,0,200,40Zm0,64H152V56h48v48Zm-96,32H56a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V152A16,16,0,0,0,104,136Zm0,64H56V152h48v48Z"/></g>"#;

/// A screen's diagonal.
/// Phosphor's `arrows-out-simple`.
const DIAGONAL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,48V208a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V48A16,16,0,0,1,48,32H208A16,16,0,0,1,224,48Z"/><path fill="currentColor" d="M216,48V96a8,8,0,0,1-16,0V67.31l-50.34,50.35a8,8,0,0,1-11.32-11.32L188.69,56H160a8,8,0,0,1,0-16h48A8,8,0,0,1,216,48ZM106.34,138.34,56,188.69V160a8,8,0,0,0-16,0v48a8,8,0,0,0,8,8H96a8,8,0,0,0,0-16H67.31l50.35-50.34a8,8,0,0,0-11.32-11.32Z"/></g>"#;

/// Wi-Fi.
/// Phosphor's `wifi-high`.
const WIFI: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M238.2,87.69l-104,125.43a8,8,0,0,1-12.3,0L17.8,87.69a7.79,7.79,0,0,1,1.31-11.21,180.75,180.75,0,0,1,217.78,0A7.79,7.79,0,0,1,238.2,87.69Z"/><path fill="currentColor" d="M247.89,80.91a15.93,15.93,0,0,0-6.17-10.81A186.67,186.67,0,0,0,128,32,186.67,186.67,0,0,0,14.28,70.1,15.93,15.93,0,0,0,8.11,80.91,15.65,15.65,0,0,0,11.65,92.8l104,125.43A15.93,15.93,0,0,0,128,224h0a15.93,15.93,0,0,0,12.31-5.77l104-125.43A15.65,15.65,0,0,0,247.89,80.91ZM128,208,24.09,82.74A170.76,170.76,0,0,1,128,48,170.76,170.76,0,0,1,231.91,82.74Z"/></g>"#;

/// A screen's pixels.
/// Phosphor's `dots-nine`.
const PIXELS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,48V208a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V48A16,16,0,0,1,48,32H208A16,16,0,0,1,224,48Z"/><path fill="currentColor" d="M72,60A12,12,0,1,1,60,48,12,12,0,0,1,72,60Zm56-12a12,12,0,1,0,12,12A12,12,0,0,0,128,48Zm68,24a12,12,0,1,0-12-12A12,12,0,0,0,196,72ZM60,116a12,12,0,1,0,12,12A12,12,0,0,0,60,116Zm68,0a12,12,0,1,0,12,12A12,12,0,0,0,128,116Zm68,0a12,12,0,1,0,12,12A12,12,0,0,0,196,116ZM60,184a12,12,0,1,0,12,12A12,12,0,0,0,60,184Zm68,0a12,12,0,1,0,12,12A12,12,0,0,0,128,184Zm68,0a12,12,0,1,0,12,12A12,12,0,0,0,196,184Z"/></g>"#;

/// A joystick.
/// Phosphor's `joystick`.
const JOYSTICK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,160v48a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V160a8,8,0,0,1,8-8H208A8,8,0,0,1,216,160ZM128,88A32,32,0,1,0,96,56,32,32,0,0,0,128,88Z"/><path fill="currentColor" d="M208,144H136V95.19a40,40,0,1,0-16,0V144H48a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V160A16,16,0,0,0,208,144ZM104,56a24,24,0,1,1,24,24A24,24,0,0,1,104,56ZM208,208H48V160H208v48Zm-40-96h32a8,8,0,0,1,0,16H168a8,8,0,0,1,0-16Z"/></g>"#;

/// A disc.
/// Phosphor's `disc`.
const DISC: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-28.12-67.88l-45.25,45.25h0A32,32,0,1,0,160,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm39.2,96a39.77,39.77,0,0,0-5.84-14l34.23-34.24a87.54,87.54,0,0,1,20,48.28ZM152,128a24,24,0,1,1-24-24A24,24,0,0,1,152,128Zm-24,88A88,88,0,1,1,184.28,60.4L150,94.64A40,40,0,1,0,167.2,136h48.43A88.11,88.11,0,0,1,128,216Z"/></g>"#;

/// A sensor in its frame.
/// Phosphor's `scan`.
const SENSOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,80v96H80V80Z"/><path fill="currentColor" d="M224,40V80a8,8,0,0,1-16,0V48H176a8,8,0,0,1,0-16h40A8,8,0,0,1,224,40ZM80,208H48V176a8,8,0,0,0-16,0v40a8,8,0,0,0,8,8H80a8,8,0,0,0,0-16Zm136-40a8,8,0,0,0-8,8v32H176a8,8,0,0,0,0,16h40a8,8,0,0,0,8-8V176A8,8,0,0,0,216,168ZM40,88a8,8,0,0,0,8-8V48H80a8,8,0,0,0,0-16H40a8,8,0,0,0-8,8V80A8,8,0,0,0,40,88ZM80,72h96a8,8,0,0,1,8,8v96a8,8,0,0,1-8,8H80a8,8,0,0,1-8-8V80A8,8,0,0,1,80,72Zm8,96h80V88H88Z"/></g>"#;

/// A lens and its mount.
/// Phosphor's `target`.
const LENS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,128a48,48,0,1,1-48-48A48,48,0,0,1,176,128Z"/><path fill="currentColor" d="M221.87,83.16A104.1,104.1,0,1,1,195.67,49l22.67-22.68a8,8,0,0,1,11.32,11.32l-96,96a8,8,0,0,1-11.32-11.32l27.72-27.72a40,40,0,1,0,17.87,31.09,8,8,0,1,1,16-.9,56,56,0,1,1-22.38-41.65L184.3,60.39a87.88,87.88,0,1,0,23.13,29.67,8,8,0,0,1,14.44-6.9Z"/></g>"#;

/// A shutter's blades.
/// Phosphor's `aperture`.
const SHUTTER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M195.88,60.12a96,96,0,1,0,0,135.76A96,96,0,0,0,195.88,60.12Zm-55.34,103h0l-36.68-6.69h0L91.32,121.3l24.14-28.41h0l36.68,6.69,12.54,35.12Z"/><path fill="currentColor" d="M201.54,54.46A104,104,0,0,0,54.46,201.54,104,104,0,0,0,201.54,54.46ZM190.23,65.78a88.18,88.18,0,0,1,11,13.48L167.55,119,139.63,40.78A87.34,87.34,0,0,1,190.23,65.78ZM155.59,133l-18.16,21.37-27.59-5L100.41,123l18.16-21.37,27.59,5ZM65.77,65.78a87.34,87.34,0,0,1,56.66-25.59l17.51,49L58.3,74.32A88,88,0,0,1,65.77,65.78ZM46.65,161.54a88.41,88.41,0,0,1,2.53-72.62l51.21,9.35Zm19.12,28.68a88.18,88.18,0,0,1-11-13.48L88.45,137l27.92,78.18A87.34,87.34,0,0,1,65.77,190.22Zm124.46,0a87.34,87.34,0,0,1-56.66,25.59l-17.51-49,81.64,14.91A88,88,0,0,1,190.23,190.22Zm-34.62-32.49,53.74-63.27a88.41,88.41,0,0,1-2.53,72.62Z"/></g>"#;

/// Bluetooth.
/// Phosphor's `bluetooth`.
const BLUETOOTH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,80l-64,48V32ZM128,224l64-48-64-48Z"/><path fill="currentColor" d="M196.8,169.6,141.33,128,196.8,86.4a8,8,0,0,0,0-12.8l-64-48A8,8,0,0,0,120,32v80L68.8,73.6a8,8,0,0,0-9.6,12.8L114.67,128,59.2,169.6a8,8,0,1,0,9.6,12.8L120,144v80a8,8,0,0,0,12.8,6.4l64-48a8,8,0,0,0,0-12.8ZM136,48l42.67,32L136,112Zm0,160V144l42.67,32Z"/></g>"#;

/// A loudspeaker, silenced.
/// Phosphor's `speaker-x`.
const MUTE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M80,88v80H32a8,8,0,0,1-8-8V96a8,8,0,0,1,8-8Z"/><path fill="currentColor" d="M155.51,24.81a8,8,0,0,0-8.42.88L77.25,80H32A16,16,0,0,0,16,96v64a16,16,0,0,0,16,16H77.25l69.84,54.31A8,8,0,0,0,160,224V32A8,8,0,0,0,155.51,24.81ZM32,96H72v64H32ZM144,207.64,88,164.07V91.94l56-43.58Zm101.66-61.3a8,8,0,0,1-11.32,11.32L216,139.31l-18.34,18.35a8,8,0,0,1-11.32-11.32L204.69,128l-18.35-18.34a8,8,0,0,1,11.32-11.32L216,116.69l18.34-18.35a8,8,0,0,1,11.32,11.32L227.31,128Z"/></g>"#;

/// An open padlock.
/// Phosphor's `lock-simple-open`.
const OPEN_LOCK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,96V208a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V96a8,8,0,0,1,8-8H208A8,8,0,0,1,216,96Z"/><path fill="currentColor" d="M208,80H96V56a32,32,0,0,1,32-32c15.37,0,29.2,11,32.16,25.59a8,8,0,0,0,15.68-3.18C171.32,24.15,151.2,8,128,8A48.05,48.05,0,0,0,80,56V80H48A16,16,0,0,0,32,96V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V96A16,16,0,0,0,208,80Zm0,128H48V96H208V208Z"/></g>"#;

/// A swatch of material.
/// Phosphor's `swatches`.
const TEXTURE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M135.88,43.11l-25,143.14a35.71,35.71,0,0,1-41.34,29.2h0a36,36,0,0,1-28.95-41.71l25-143.13a8,8,0,0,1,9.19-6.49l54.67,9.73A8,8,0,0,1,135.88,43.11Z"/><path fill="currentColor" d="M88,180a12,12,0,1,1-12-12A12,12,0,0,1,88,180Zm152-23.81V208a16,16,0,0,1-16,16H76a46.36,46.36,0,0,1-7.94-.68,44,44,0,0,1-35.43-50.95l25-143.13a15.94,15.94,0,0,1,18.47-13L130.84,26a16,16,0,0,1,12.92,18.52l-12.08,69L199.49,89a16,16,0,0,1,20.45,9.52L239,150.69A18.35,18.35,0,0,1,240,156.19ZM103,184.87,128,41.74,73.46,32l-25,143.1A28,28,0,0,0,70.9,207.57,27.29,27.29,0,0,0,91.46,203,27.84,27.84,0,0,0,103,184.87ZM116.78,195,224,156.11,204.92,104,128.5,131.7l-9.78,55.92A44.63,44.63,0,0,1,116.78,195ZM224,173.12,127.74,208H224Z"/></g>"#;

/// A ruler.
/// Phosphor's `ruler`.
const RULER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M229.66,90.34,90.34,229.66a8,8,0,0,1-11.31,0L26.34,177a8,8,0,0,1,0-11.31L165.66,26.34a8,8,0,0,1,11.31,0L229.66,79A8,8,0,0,1,229.66,90.34Z"/><path fill="currentColor" d="M235.32,73.37,182.63,20.69a16,16,0,0,0-22.63,0L20.68,160a16,16,0,0,0,0,22.63l52.69,52.68a16,16,0,0,0,22.63,0L235.32,96A16,16,0,0,0,235.32,73.37ZM84.68,224,32,171.31l32-32,26.34,26.35a8,8,0,0,0,11.32-11.32L75.31,128,96,107.31l26.34,26.35a8,8,0,0,0,11.32-11.32L107.31,96,128,75.31l26.34,26.35a8,8,0,0,0,11.32-11.32L139.31,64l32-32L224,84.69Z"/></g>"#;

/// The registered trade mark.
/// Phosphor's `trademark-registered`.
const BRAND: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm23.09-75.79A32,32,0,0,0,136,80H104a8,8,0,0,0-8,8v80a8,8,0,0,0,16,0V144h22.39l19,28.44a8,8,0,0,0,13.32-8.88ZM112,96h24a16,16,0,0,1,0,32H112Z"/></g>"#;

/// An energy label's arrows.
/// Phosphor's `lightning-a`.
const ENERGY: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M72.8,216,88,152,32,126.86,127.2,24,112,88l56,25.14Z"/><path fill="currentColor" d="M175.84,111.54a8,8,0,0,0-4.56-5.7l-50-22.43L135,25.85a8,8,0,0,0-13.65-7.28L26.13,121.42a8,8,0,0,0,2.59,12.73l50,22.44L65,214.15a8,8,0,0,0,13.65,7.28l95.2-102.85A8,8,0,0,0,175.84,111.54ZM87.62,188.21l8.16-34.36a8,8,0,0,0-4.5-9.15L45.43,124.12l66.95-72.33-8.16,34.36a8,8,0,0,0,4.5,9.15l45.84,20.58Zm151.53,24.21-36-72a8,8,0,0,0-14.31,0l-36,72a8,8,0,0,0,14.31,7.16L176.94,200h38.11l9.79,19.58A8,8,0,0,0,232,224a8,8,0,0,0,7.15-11.58ZM184.94,184,196,161.89,207.05,184Z"/></g>"#;

/// A measuring jug.
/// Phosphor's `jar`.
const JUG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,88V200a32,32,0,0,1-32,32H80a32,32,0,0,1-32-32V88A32,32,0,0,1,80,56h96A32,32,0,0,1,208,88Z"/><path fill="currentColor" d="M184,48.81V32a16,16,0,0,0-16-16H88A16,16,0,0,0,72,32V48.81A40.05,40.05,0,0,0,40,88V200a40,40,0,0,0,40,40h96a40,40,0,0,0,40-40V88A40.05,40.05,0,0,0,184,48.81ZM168,48H152V32h16Zm-48,0V32h16V48ZM104,32V48H88V32Zm96,168a24,24,0,0,1-24,24H80a24,24,0,0,1-24-24V88A24,24,0,0,1,80,64h96a24,24,0,0,1,24,24Z"/></g>"#;

/// A clock: reserving a listing, and a listing about to expire (notifications.rs).
/// Phosphor's `clock`.
pub const CLOCK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm64-88a8,8,0,0,1-8,8H128a8,8,0,0,1-8-8V72a8,8,0,0,1,16,0v48h48A8,8,0,0,1,192,128Z"/></g>"#;

/// A document.
/// Phosphor's `file-text`.
const DOCUMENT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,88H152V32Z"/><path fill="currentColor" d="M213.66,82.34l-56-56A8,8,0,0,0,152,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V88A8,8,0,0,0,213.66,82.34ZM160,51.31,188.69,80H160ZM200,216H56V40h88V88a8,8,0,0,0,8,8h48V216Zm-32-80a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,136Zm0,32a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,168Z"/></g>"#;

/// A star.
/// Phosphor's `star`.
const STAR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M229.06,108.79l-48.7,42,14.88,62.79a8.4,8.4,0,0,1-12.52,9.17L128,189.09,73.28,222.74a8.4,8.4,0,0,1-12.52-9.17l14.88-62.79-48.7-42A8.46,8.46,0,0,1,31.73,94L95.64,88.8l24.62-59.6a8.36,8.36,0,0,1,15.48,0l24.62,59.6L224.27,94A8.46,8.46,0,0,1,229.06,108.79Z"/><path fill="currentColor" d="M239.18,97.26A16.38,16.38,0,0,0,224.92,86l-59-4.76L143.14,26.15a16.36,16.36,0,0,0-30.27,0L90.11,81.23,31.08,86a16.46,16.46,0,0,0-9.37,28.86l45,38.83L53,211.75a16.38,16.38,0,0,0,24.5,17.82L128,198.49l50.53,31.08A16.4,16.4,0,0,0,203,211.75l-13.76-58.07,45-38.83A16.43,16.43,0,0,0,239.18,97.26Zm-15.34,5.47-48.7,42a8,8,0,0,0-2.56,7.91l14.88,62.8a.37.37,0,0,1-.17.48c-.18.14-.23.11-.38,0l-54.72-33.65a8,8,0,0,0-8.38,0L69.09,215.94c-.15.09-.19.12-.38,0a.37.37,0,0,1-.17-.48l14.88-62.8a8,8,0,0,0-2.56-7.91l-48.7-42c-.12-.1-.23-.19-.13-.5s.18-.27.33-.29l63.92-5.16A8,8,0,0,0,103,91.86l24.62-59.61c.08-.17.11-.25.35-.25s.27.08.35.25L153,91.86a8,8,0,0,0,6.75,4.92l63.92,5.16c.15,0,.24,0,.33.29S224,102.63,223.84,102.73Z"/></g>"#;

/// Two arrows, each round to the other.
/// Phosphor's `arrows-left-right`.
const SHIFTS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,80v96H48V80Z"/><path fill="currentColor" d="M213.66,181.66l-32,32a8,8,0,0,1-11.32-11.32L188.69,184H48a8,8,0,0,1,0-16H188.69l-18.35-18.34a8,8,0,0,1,11.32-11.32l32,32A8,8,0,0,1,213.66,181.66Zm-139.32-64a8,8,0,0,0,11.32-11.32L67.31,88H208a8,8,0,0,0,0-16H67.31L85.66,53.66A8,8,0,0,0,74.34,42.34l-32,32a8,8,0,0,0,0,11.32Z"/></g>"#;

/// A folded map.
/// Phosphor's `map-trifold`.
const MAP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M160,72V216L96,184V40Z"/><path fill="currentColor" d="M228.92,49.69a8,8,0,0,0-6.86-1.45L160.93,63.52,99.58,32.84a8,8,0,0,0-5.52-.6l-64,16A8,8,0,0,0,24,56V200a8,8,0,0,0,9.94,7.76l61.13-15.28,61.35,30.68A8.15,8.15,0,0,0,160,224a8,8,0,0,0,1.94-.24l64-16A8,8,0,0,0,232,200V56A8,8,0,0,0,228.92,49.69ZM104,52.94l48,24V203.06l-48-24ZM40,62.25l48-12v127.5l-48,12Zm176,131.5-48,12V78.25l48-12Z"/></g>"#;

/// A receipt.
/// Phosphor's `receipt`.
const RECEIPT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V208l-32-16-32,16-32-16L96,208,64,192,32,208V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z"/><path fill="currentColor" d="M72,104a8,8,0,0,1,8-8h96a8,8,0,0,1,0,16H80A8,8,0,0,1,72,104Zm8,40h96a8,8,0,0,0,0-16H80a8,8,0,0,0,0,16ZM232,56V208a8,8,0,0,1-11.58,7.15L192,200.94l-28.42,14.21a8,8,0,0,1-7.16,0L128,200.94,99.58,215.15a8,8,0,0,1-7.16,0L64,200.94,35.58,215.15A8,8,0,0,1,24,208V56A16,16,0,0,1,40,40H216A16,16,0,0,1,232,56Zm-16,0H40V195.06l20.42-10.22a8,8,0,0,1,7.16,0L96,199.06l28.42-14.22a8,8,0,0,1,7.16,0L160,199.06l28.42-14.22a8,8,0,0,1,7.16,0L216,195.06Z"/></g>"#;

/// A person.
/// Phosphor's `user`.
const PERSON: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,96a64,64,0,1,1-64-64A64,64,0,0,1,192,96Z"/><path fill="currentColor" d="M230.92,212c-15.23-26.33-38.7-45.21-66.09-54.16a72,72,0,1,0-73.66,0C63.78,166.78,40.31,185.66,25.08,212a8,8,0,1,0,13.85,8c18.84-32.56,52.14-52,89.07-52s70.23,19.44,89.07,52a8,8,0,1,0,13.85-8ZM72,96a56,56,0,1,1,56,56A56.06,56.06,0,0,1,72,96Z"/></g>"#;

/// A tape measure.
/// Phosphor's `pencil-ruler`.
const TAPE_MEASURE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M40,184h64v24a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8ZM208,40H160a8,8,0,0,0-8,8V208a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V48A8,8,0,0,0,208,40Z"/><path fill="currentColor" d="M208,32H160a16,16,0,0,0-16,16V208a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Zm0,176H160V176h24a8,8,0,0,0,0-16H160V136h24a8,8,0,0,0,0-16H160V96h24a8,8,0,0,0,0-16H160V48h48V208ZM77.66,26.34a8,8,0,0,0-11.32,0l-32,32A8,8,0,0,0,32,64V208a16,16,0,0,0,16,16H96a16,16,0,0,0,16-16V64a8,8,0,0,0-2.34-5.66ZM48,176V80H64v96ZM80,80H96v96H80ZM72,43.31,92.69,64H51.31ZM48,208V192H96v16Z"/></g>"#;

/// Two footprints.
/// Phosphor's `footprints`.
const FOOTPRINTS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M152,192h56v12a28,28,0,0,1-56,0ZM48,180a28,28,0,0,0,56,0V168H48ZM76,24c-24.52,0-51.46,80-25.26,112h50.52C127.46,104,100.52,24,76,24ZM205.26,160c26.2-32-.74-112-25.26-112s-51.46,80-25.26,112Z"/><path fill="currentColor" d="M208.06,184H152a8,8,0,0,0-8,8v12a36,36,0,0,0,72.05,0V192A8,8,0,0,0,208.06,184Zm-8,20a20,20,0,0,1-40,0v-4h40ZM104,160h-56a8,8,0,0,0-8,8v12A36,36,0,0,0,112,180V168A8,8,0,0,0,104,160Zm-8,20a20,20,0,0,1-40,0v-4H96ZM76,16C64.36,16,53.07,26.31,44.2,45c-13.93,29.38-18.56,73,.29,96a8,8,0,0,0,6.2,2.93h50.55a8,8,0,0,0,6.2-2.93c18.85-23,14.22-66.65.29-96C98.85,26.31,87.57,16,76,16ZM97.15,128H54.78c-11.4-18.1-7.21-52.7,3.89-76.11C65.14,38.22,72.17,32,76,32s10.82,6.22,17.3,19.89C104.36,75.3,108.55,109.9,97.15,128Zm57.61,40h50.55a8,8,0,0,0,6.2-2.93c18.85-23,14.22-66.65.29-96C202.93,50.31,191.64,40,180,40s-22.89,10.31-31.77,29c-13.93,29.38-18.56,73,.29,96A8,8,0,0,0,154.76,168Zm8-92.11C169.22,62.22,176.25,56,180,56s10.81,6.22,17.29,19.89c11.1,23.41,15.29,58,3.89,76.11H158.85C147.45,133.9,151.64,99.3,162.74,75.89Z"/></g>"#;

/// A spoked wheel.
/// Phosphor's `tire`.
const WHEEL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M164,224H92c-28.72,0-52-43-52-96S63.28,32,92,32h72c-28.72,0-52,43-52,96S135.28,224,164,224Zm4-136c-8.84,0-16,17.91-16,40s7.16,40,16,40,16-17.91,16-40S176.84,88,168,88Z"/><path fill="currentColor" d="M149.26,159.26C155.09,173.82,163.5,176,168,176s12.91-2.18,18.74-16.74c3.39-8.48,5.26-19.58,5.26-31.26s-1.87-22.78-5.26-31.26C180.91,82.18,172.5,80,168,80s-12.91,2.18-18.74,16.74C145.87,105.22,144,116.32,144,128S145.87,150.78,149.26,159.26ZM168,96.2c2.62,2.06,8,13,8,31.8s-5.38,29.74-8,31.8c-2.62-2.06-8-13-8-31.8S165.38,98.26,168,96.2ZM232,216H196.41C213.12,197.73,224,165.47,224,128c0-58.32-26.35-104-60-104H92C58.35,24,32,69.68,32,128S58.35,232,92,232H232a8,8,0,0,0,0-16ZM193.74,63.93C202.93,80.91,208,103.67,208,128s-5.07,47.09-14.26,64.07C185.38,207.5,174.82,216,164,216s-21.38-8.5-29.74-23.93C125.07,175.09,120,152.33,120,128s5.07-47.09,14.26-64.07C142.62,48.5,153.18,40,164,40S185.38,48.5,193.74,63.93ZM48,128c0-2.5.07-5,.17-7.44L80,97.83l24.43,17.45c-.28,4.16-.43,8.41-.43,12.72a179.89,179.89,0,0,0,3.07,33.5l-22.42-16a8,8,0,0,0-9.3,0l-23.74,17A161,161,0,0,1,48,128ZM62.26,63.93C70.62,48.5,81.18,40,92,40h39.59c-11.9,13-20.84,33.12-25,57.16L84.65,81.49a8,8,0,0,0-9.3,0L50.49,99.25C52.85,86,56.83,74,62.26,63.93Zm0,128.14a100.08,100.08,0,0,1-5.94-13.32L80,161.83l33.94,24.24c4.6,12,10.6,22.22,17.65,29.93H92C81.18,216,70.62,207.5,62.26,192.07Z"/></g>"#;

/// A sound wave.
/// Phosphor's `waveform`.
const SOUND_WAVE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,96v64H48V96Z"/><path fill="currentColor" d="M56,96v64a8,8,0,0,1-16,0V96a8,8,0,0,1,16,0ZM88,24a8,8,0,0,0-8,8V224a8,8,0,0,0,16,0V32A8,8,0,0,0,88,24Zm40,32a8,8,0,0,0-8,8V192a8,8,0,0,0,16,0V64A8,8,0,0,0,128,56Zm40,32a8,8,0,0,0-8,8v64a8,8,0,0,0,16,0V96A8,8,0,0,0,168,88Zm40-16a8,8,0,0,0-8,8v96a8,8,0,0,0,16,0V80A8,8,0,0,0,208,72Z"/></g>"#;

/// A cake and its candle.
/// Phosphor's `cake`.
const CAKE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M104,48c0-24,24-40,24-40s24,16,24,40a24,24,0,0,1-48,0ZM208,96H48a16,16,0,0,0-16,16v23.33c0,17.44,13.67,32.18,31.1,32.66A32,32,0,0,0,96,136a32,32,0,0,0,64,0,32,32,0,0,0,32.9,32c17.43-.48,31.1-15.22,31.1-32.66V112A16,16,0,0,0,208,96Z"/><path fill="currentColor" d="M232,112a24,24,0,0,0-24-24H136V79a32.06,32.06,0,0,0,24-31c0-28-26.44-45.91-27.56-46.66a8,8,0,0,0-8.88,0C122.44,2.09,96,20,96,48a32.06,32.06,0,0,0,24,31v9H48a24,24,0,0,0-24,24v23.33a40.84,40.84,0,0,0,8,24.24V200a24,24,0,0,0,24,24H200a24,24,0,0,0,24-24V159.57a40.84,40.84,0,0,0,8-24.24ZM112,48c0-13.57,10-24.46,16-29.79,6,5.33,16,16.22,16,29.79a16,16,0,0,1-32,0ZM40,112a8,8,0,0,1,8-8H208a8,8,0,0,1,8,8v23.33c0,13.25-10.46,24.31-23.32,24.66A24,24,0,0,1,168,136a8,8,0,0,0-16,0,24,24,0,0,1-48,0,8,8,0,0,0-16,0,24,24,0,0,1-24.68,24C50.46,159.64,40,148.58,40,135.33Zm160,96H56a8,8,0,0,1-8-8V172.56A38.77,38.77,0,0,0,62.88,176a39.69,39.69,0,0,0,29-11.31A40.36,40.36,0,0,0,96,160a40,40,0,0,0,64,0,40.36,40.36,0,0,0,4.13,4.67A39.67,39.67,0,0,0,192,176c.38,0,.76,0,1.14,0A38.77,38.77,0,0,0,208,172.56V200A8,8,0,0,1,200,208Z"/></g>"#;

/// The signs of the sexes, together.
/// Phosphor's `gender-intersex`.
const SEXES: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,112a56,56,0,1,1-56-56A56,56,0,0,1,176,112Z"/><path fill="currentColor" d="M208,24H168a8,8,0,0,0,0,16h20.69L163.54,65.15A64,64,0,1,0,112,175.48V192H88a8,8,0,0,0,0,16h24v24a8,8,0,0,0,16,0V208h24a8,8,0,0,0,0-16H128V175.48a63.92,63.92,0,0,0,45.84-98L200,51.31V72a8,8,0,0,0,16,0V32A8,8,0,0,0,208,24ZM120,160a48,48,0,1,1,48-48A48.05,48.05,0,0,1,120,160Z"/></g>"#;

/// A syringe.
/// Phosphor's `syringe`.
const SYRINGE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M196,116l-89.66,89.66a8,8,0,0,1-5.65,2.34H48V155.31a8,8,0,0,1,2.34-5.65L140,60Z"/><path fill="currentColor" d="M237.66,66.34l-48-48a8,8,0,0,0-11.32,11.32L196.69,48,168,76.69,133.66,42.34a8,8,0,0,0-11.32,11.32L128.69,60l-84,84A15.86,15.86,0,0,0,40,155.31v49.38L18.34,226.34a8,8,0,0,0,11.32,11.32L51.31,216h49.38A15.86,15.86,0,0,0,112,211.31l84-84,6.34,6.35a8,8,0,0,0,11.32-11.32L179.31,88,208,59.31l18.34,18.35a8,8,0,0,0,11.32-11.32ZM100.69,200H56V155.31l18-18,20.34,20.35a8,8,0,0,0,11.32-11.32L85.31,126,98,113.31l20.34,20.35a8,8,0,0,0,11.32-11.32L109.31,102,140,71.31,184.69,116Z"/></g>"#;

/// A rosette.
/// Phosphor's `seal-check`.
const ROSETTE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,128c0,12.51-17.82,21.95-22.68,33.69-4.68,11.32,1.42,30.65-7.78,39.85s-28.53,3.1-39.85,7.78C150,214.18,140.5,232,128,232s-22-17.82-33.69-22.68c-11.32-4.68-30.65,1.42-39.85-7.78s-3.1-28.53-7.78-39.85C41.82,150,24,140.5,24,128s17.82-22,22.68-33.69C51.36,83,45.26,63.66,54.46,54.46S83,51.36,94.31,46.68C106.05,41.82,115.5,24,128,24S150,41.82,161.69,46.68c11.32,4.68,30.65-1.42,39.85,7.78s3.1,28.53,7.78,39.85C214.18,106.05,232,115.5,232,128Z"/><path fill="currentColor" d="M225.86,102.82c-3.77-3.94-7.67-8-9.14-11.57-1.36-3.27-1.44-8.69-1.52-13.94-.15-9.76-.31-20.82-8-28.51s-18.75-7.85-28.51-8c-5.25-.08-10.67-.16-13.94-1.52-3.56-1.47-7.63-5.37-11.57-9.14C146.28,23.51,138.44,16,128,16s-18.27,7.51-25.18,14.14c-3.94,3.77-8,7.67-11.57,9.14C88,40.64,82.56,40.72,77.31,40.8c-9.76.15-20.82.31-28.51,8S41,67.55,40.8,77.31c-.08,5.25-.16,10.67-1.52,13.94-1.47,3.56-5.37,7.63-9.14,11.57C23.51,109.72,16,117.56,16,128s7.51,18.27,14.14,25.18c3.77,3.94,7.67,8,9.14,11.57,1.36,3.27,1.44,8.69,1.52,13.94.15,9.76.31,20.82,8,28.51s18.75,7.85,28.51,8c5.25.08,10.67.16,13.94,1.52,3.56,1.47,7.63,5.37,11.57,9.14C109.72,232.49,117.56,240,128,240s18.27-7.51,25.18-14.14c3.94-3.77,8-7.67,11.57-9.14,3.27-1.36,8.69-1.44,13.94-1.52,9.76-.15,20.82-.31,28.51-8s7.85-18.75,8-28.51c.08-5.25.16-10.67,1.52-13.94,1.47-3.56,5.37-7.63,9.14-11.57C232.49,146.28,240,138.44,240,128S232.49,109.73,225.86,102.82Zm-11.55,39.29c-4.79,5-9.75,10.17-12.38,16.52-2.52,6.1-2.63,13.07-2.73,19.82-.1,7-.21,14.33-3.32,17.43s-10.39,3.22-17.43,3.32c-6.75.1-13.72.21-19.82,2.73-6.35,2.63-11.52,7.59-16.52,12.38S132,224,128,224s-9.15-4.92-14.11-9.69-10.17-9.75-16.52-12.38c-6.1-2.52-13.07-2.63-19.82-2.73-7-.1-14.33-.21-17.43-3.32s-3.22-10.39-3.32-17.43c-.1-6.75-.21-13.72-2.73-19.82-2.63-6.35-7.59-11.52-12.38-16.52S32,132,32,128s4.92-9.15,9.69-14.11,9.75-10.17,12.38-16.52c2.52-6.1,2.63-13.07,2.73-19.82.1-7,.21-14.33,3.32-17.43S70.51,56.9,77.55,56.8c6.75-.1,13.72-.21,19.82-2.73,6.35-2.63,11.52-7.59,16.52-12.38S124,32,128,32s9.15,4.92,14.11,9.69,10.17,9.75,16.52,12.38c6.1,2.52,13.07,2.63,19.82,2.73,7,.1,14.33.21,17.43,3.32s3.22,10.39,3.32,17.43c.1,6.75.21,13.72,2.73,19.82,2.63,6.35,7.59,11.52,12.38,16.52S224,124,224,128,219.08,137.15,214.31,142.11ZM173.66,98.34a8,8,0,0,1,0,11.32l-56,56a8,8,0,0,1-11.32,0l-24-24a8,8,0,0,1,11.32-11.32L112,148.69l50.34-50.35A8,8,0,0,1,173.66,98.34Z"/></g>"#;

/// A stopwatch.
/// Phosphor's `timer`.
const STOPWATCH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,136a88,88,0,1,1-88-88A88,88,0,0,1,216,136Z"/><path fill="currentColor" d="M128,40a96,96,0,1,0,96,96A96.11,96.11,0,0,0,128,40Zm0,176a80,80,0,1,1,80-80A80.09,80.09,0,0,1,128,216ZM173.66,90.34a8,8,0,0,1,0,11.32l-40,40a8,8,0,0,1-11.32-11.32l40-40A8,8,0,0,1,173.66,90.34ZM96,16a8,8,0,0,1,8-8h48a8,8,0,0,1,0,16H104A8,8,0,0,1,96,16Z"/></g>"#;

/// A sine wave.
/// Phosphor's `wave-sine`.
const SINE_WAVE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,128c-52,110.85-78,55.43-104,0ZM24,128H128C102,72.57,76,17.15,24,128Z"/><path fill="currentColor" d="M239.24,131.4c-22,46.8-41.4,68.6-61.2,68.6-25.1,0-40.73-33.32-57.28-68.6C107.7,103.56,92.9,72,78,72c-16.4,0-36.31,37.21-46.72,59.4a8,8,0,0,1-14.48-6.8C38.71,77.8,58.16,56,78,56c25.1,0,40.73,33.32,57.28,68.6C148.3,152.44,163.1,184,178,184c16.4,0,36.31-37.21,46.72-59.4a8,8,0,0,1,14.48,6.8Z"/></g>"#;

/// An engine.
/// Phosphor's `engine`.
const ENGINE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M248,120v48a8,8,0,0,1-8,8H224l-37.66,37.66a8,8,0,0,1-5.65,2.34H103.31a8,8,0,0,1-5.65-2.34L58.34,174.34A8,8,0,0,1,56,168.69V80a8,8,0,0,1,8-8H180.69a8,8,0,0,1,5.65,2.34L224,112h16A8,8,0,0,1,248,120Z"/><path fill="currentColor" d="M240,104H227.31L192,68.69A15.86,15.86,0,0,0,180.69,64H140V40h24a8,8,0,0,0,0-16H100a8,8,0,0,0,0,16h24V64H64A16,16,0,0,0,48,80v52H24V108a8,8,0,0,0-16,0v64a8,8,0,0,0,16,0V148H48v20.69A15.86,15.86,0,0,0,52.69,180L92,219.31A15.86,15.86,0,0,0,103.31,224h77.38A15.86,15.86,0,0,0,192,219.31L227.31,184H240a16,16,0,0,0,16-16V120A16,16,0,0,0,240,104Zm0,64H224a8,8,0,0,0-5.66,2.34L180.69,208H103.31L64,168.69V80H180.69l37.65,37.66A8,8,0,0,0,224,120h16Z"/></g>"#;

/// A computer's tower.
/// Phosphor's `desktop-tower`.
const TOWER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,48V208a8,8,0,0,1-8,8H152a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8h80A8,8,0,0,1,240,48Z"/><path fill="currentColor" d="M216,72a8,8,0,0,1-8,8H176a8,8,0,0,1,0-16h32A8,8,0,0,1,216,72Zm-8,24H176a8,8,0,0,0,0,16h32a8,8,0,0,0,0-16Zm40-48V208a16,16,0,0,1-16,16H152a16,16,0,0,1-16-16V192H96v16h16a8,8,0,0,1,0,16H64a8,8,0,0,1,0-16H80V192H32A24,24,0,0,1,8,168V96A24,24,0,0,1,32,72H136V48a16,16,0,0,1,16-16h80A16,16,0,0,1,248,48ZM136,176V88H32a8,8,0,0,0-8,8v72a8,8,0,0,0,8,8Zm96,32V48H152V208h80Zm-40-40a12,12,0,1,0,12,12A12,12,0,0,0,192,168Z"/></g>"#;

/// A door.
/// Phosphor's `door-open`.
const DOOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,40V224H168V32h24A8,8,0,0,1,200,40Z"/><path fill="currentColor" d="M232,216H208V40a16,16,0,0,0-16-16H64A16,16,0,0,0,48,40V216H24a8,8,0,0,0,0,16H232a8,8,0,0,0,0-16Zm-40,0H176V40h16ZM64,40h96V216H64Zm80,92a12,12,0,1,1-12-12A12,12,0,0,1,144,132Z"/></g>"#;

// ---------------------------------------------------------------- categories

/// An eye: how many times a listing was seen (listing_card.html, listing.html, my_listings.html).
/// Phosphor's `eye`.
pub const EYE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,56C48,56,16,128,16,128s32,72,112,72,112-72,112-72S208,56,128,56Zm0,112a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z"/><path fill="currentColor" d="M247.31,124.76c-.35-.79-8.82-19.58-27.65-38.41C194.57,61.26,162.88,48,128,48S61.43,61.26,36.34,86.35C17.51,105.18,9,124,8.69,124.76a8,8,0,0,0,0,6.5c.35.79,8.82,19.57,27.65,38.4C61.43,194.74,93.12,208,128,208s66.57-13.26,91.66-38.34c18.83-18.83,27.3-37.61,27.65-38.4A8,8,0,0,0,247.31,124.76ZM128,192c-30.78,0-57.67-11.19-79.93-33.25A133.47,133.47,0,0,1,25,128,133.33,133.33,0,0,1,48.07,97.25C70.33,75.19,97.22,64,128,64s57.67,11.19,79.93,33.25A133.46,133.46,0,0,1,231.05,128C223.84,141.46,192.43,192,128,192Zm0-112a48,48,0,1,0,48,48A48.05,48.05,0,0,0,128,80Zm0,80a32,32,0,1,1,32-32A32,32,0,0,1,128,160Z"/></g>"#;

/// A heart: the listings a visitor keeps (the header's button in base.html, the welcome page's at
/// the end of its categories in home.html, heart.html over a listing's photo), and how many keep a
/// seller's listing (my_listings.html).
/// Phosphor's `heart`.
pub const HEART: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,102c0,66-104,122-104,122S24,168,24,102A54,54,0,0,1,78,48c22.59,0,41.94,12.31,50,32,8.06-19.69,27.41-32,50-32A54,54,0,0,1,232,102Z"/><path fill="currentColor" d="M178,40c-20.65,0-38.73,8.88-50,23.89C116.73,48.88,98.65,40,78,40a62.07,62.07,0,0,0-62,62c0,70,103.79,126.66,108.21,129a8,8,0,0,0,7.58,0C136.21,228.66,240,172,240,102A62.07,62.07,0,0,0,178,40ZM128,214.8C109.74,204.16,32,155.69,32,102A46.06,46.06,0,0,1,78,56c19.45,0,35.78,10.36,42.6,27a8,8,0,0,0,14.8,0c6.82-16.67,23.15-27,42.6-27a46.06,46.06,0,0,1,46,46C224,155.61,146.24,204.15,128,214.8Z"/></g>"#;

/// A flag: reporting a listing (listing.html).
/// Phosphor's `flag`.
pub const FLAG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V176c-64,55.43-112-55.43-176,0V56C112,.57,160,111.43,224,56Z"/><path fill="currentColor" d="M42.76,50A8,8,0,0,0,40,56V224a8,8,0,0,0,16,0V179.77c26.79-21.16,49.87-9.75,76.45,3.41,16.4,8.11,34.06,16.85,53,16.85,13.93,0,28.54-4.75,43.82-18a8,8,0,0,0,2.76-6V56A8,8,0,0,0,218.76,50c-28,24.23-51.72,12.49-79.21-1.12C111.07,34.76,78.78,18.79,42.76,50ZM216,172.25c-26.79,21.16-49.87,9.74-76.45-3.41-25-12.35-52.81-26.13-83.55-8.4V59.79c26.79-21.16,49.87-9.75,76.45,3.4,25,12.35,52.82,26.13,83.55,8.4Z"/></g>"#;

/// A circle crossed out: blocking a seller (listing.html).
/// Phosphor's `prohibit`.
pub const BLOCK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm88,104a87.56,87.56,0,0,1-20.41,56.28L71.72,60.4A88,88,0,0,1,216,128ZM40,128A87.56,87.56,0,0,1,60.41,71.72L184.28,195.6A88,88,0,0,1,40,128Z"/></g>"#;
/// The seller's own buttons on their listing's page (listing.html): a pencil for editing it
/// ("Ndrysho"), two bars for taking it down for a while ("Çaktivizo"), a bin for deleting it
/// ("Fshi").
/// Phosphor's `pencil-simple`.
pub const PENCIL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M221.66,90.34,192,120,136,64l29.66-29.66a8,8,0,0,1,11.31,0L221.66,79A8,8,0,0,1,221.66,90.34Z"/><path fill="currentColor" d="M227.31,73.37,182.63,28.68a16,16,0,0,0-22.63,0L36.69,152A15.86,15.86,0,0,0,32,163.31V208a16,16,0,0,0,16,16H92.69A15.86,15.86,0,0,0,104,219.31L227.31,96a16,16,0,0,0,0-22.63ZM92.69,208H48V163.31l88-88L180.69,120ZM192,108.68,147.31,64l24-24L216,84.68Z"/></g>"#;
/// Phosphor's `pause`.
pub const PAUSE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,48V208a8,8,0,0,1-8,8H160a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8h40A8,8,0,0,1,208,48ZM96,40H56a8,8,0,0,0-8,8V208a8,8,0,0,0,8,8H96a8,8,0,0,0,8-8V48A8,8,0,0,0,96,40Z"/><path fill="currentColor" d="M200,32H160a16,16,0,0,0-16,16V208a16,16,0,0,0,16,16h40a16,16,0,0,0,16-16V48A16,16,0,0,0,200,32Zm0,176H160V48h40ZM96,32H56A16,16,0,0,0,40,48V208a16,16,0,0,0,16,16H96a16,16,0,0,0,16-16V48A16,16,0,0,0,96,32Zm0,176H56V48H96Z"/></g>"#;
/// Phosphor's `trash`.
pub const TRASH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,56V208a8,8,0,0,1-8,8H64a8,8,0,0,1-8-8V56Z"/><path fill="currentColor" d="M216,48H176V40a24,24,0,0,0-24-24H104A24,24,0,0,0,80,40v8H40a8,8,0,0,0,0,16h8V208a16,16,0,0,0,16,16H192a16,16,0,0,0,16-16V64h8a8,8,0,0,0,0-16ZM96,40a8,8,0,0,1,8-8h48a8,8,0,0,1,8,8v8H96Zm96,168H64V64H192ZM112,104v64a8,8,0,0,1-16,0V104a8,8,0,0,1,16,0Zm48,0v64a8,8,0,0,1-16,0V104a8,8,0,0,1,16,0Z"/></g>"#;
/// And a circle with a check for saying a listing is sold or let out ("U shit", "U dha me qira"),
/// the clock for reserving it ("Rezervo") or taking the reservation off ("Hiq rezervimin"), and
/// a triangle for putting a paused or sold one back up ("Aktivizo").
/// Phosphor's `check-circle`.
pub const SOLD: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M173.66,98.34a8,8,0,0,1,0,11.32l-56,56a8,8,0,0,1-11.32,0l-24-24a8,8,0,0,1,11.32-11.32L112,148.69l50.34-50.35A8,8,0,0,1,173.66,98.34ZM232,128A104,104,0,1,1,128,24,104.11,104.11,0,0,1,232,128Zm-16,0a88,88,0,1,0-88,88A88.1,88.1,0,0,0,216,128Z"/></g>"#;
pub const RESERVE: &str = CLOCK;
/// Phosphor's `play`.
pub const RESUME: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M228.23,134.69,84.15,222.81A8,8,0,0,1,72,216.12V39.88a8,8,0,0,1,12.15-6.69l144.08,88.12A7.82,7.82,0,0,1,228.23,134.69Z"/><path fill="currentColor" d="M232.4,114.49,88.32,26.35a16,16,0,0,0-16.2-.3A15.86,15.86,0,0,0,64,39.87V216.13A15.94,15.94,0,0,0,80,232a16.07,16.07,0,0,0,8.36-2.35L232.4,141.51a15.81,15.81,0,0,0,0-27ZM80,215.94V40l143.83,88Z"/></g>"#;
/// Two arrows round a circle: renewing a listing that expired or soon will ("Rifresko").
/// Phosphor's `arrows-clockwise`.
pub const RENEW: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,128a88,88,0,1,1-88-88A88,88,0,0,1,216,128Z"/><path fill="currentColor" d="M224,48V96a8,8,0,0,1-8,8H168a8,8,0,0,1,0-16h28.69L182.06,73.37a79.56,79.56,0,0,0-56.13-23.43h-.45A79.52,79.52,0,0,0,69.59,72.71,8,8,0,0,1,58.41,61.27a96,96,0,0,1,135,.79L208,76.69V48a8,8,0,0,1,16,0ZM186.41,183.29a80,80,0,0,1-112.47-.66L59.31,168H88a8,8,0,0,0,0-16H40a8,8,0,0,0-8,8v48a8,8,0,0,0,16,0V179.31l14.63,14.63A95.43,95.43,0,0,0,130,222.06h.53a95.36,95.36,0,0,0,67.07-27.33,8,8,0,0,0-11.18-11.44Z"/></g>"#;

/// A shield with a check: a seller's number or address verified (listing.html, profile.html).
/// Phosphor's `shield-check`.
pub const VERIFIED: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,56v56c0,96-88,120-88,120S40,208,40,112V56a8,8,0,0,1,8-8H208A8,8,0,0,1,216,56Z"/><path fill="currentColor" d="M208,40H48A16,16,0,0,0,32,56v56c0,52.72,25.52,84.67,46.93,102.19,23.06,18.86,46,25.26,47,25.53a8,8,0,0,0,4.2,0c1-.27,23.91-6.67,47-25.53C198.48,196.67,224,164.72,224,112V56A16,16,0,0,0,208,40Zm0,72c0,37.07-13.66,67.16-40.6,89.42A129.3,129.3,0,0,1,128,223.62a128.25,128.25,0,0,1-38.92-21.81C61.82,179.51,48,149.3,48,112l0-56,160,0ZM82.34,141.66a8,8,0,0,1,11.32-11.32L112,148.69l50.34-50.35a8,8,0,0,1,11.32,11.32l-56,56a8,8,0,0,1-11.32,0Z"/></g>"#;

/// A telephone's handset, for a call: the button that shows a seller's number (phone.svg), and how
/// many times it was shown (my_listings.html).
/// Phosphor's `phone`.
pub const CALL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M223.94,174.08A48.33,48.33,0,0,1,176,216,136,136,0,0,1,40,80,48.33,48.33,0,0,1,81.92,32.06a8,8,0,0,1,8.3,4.8l21.13,47.2a8,8,0,0,1-.66,7.53L89.32,117a7.93,7.93,0,0,0-.54,7.81c8.27,16.93,25.77,34.22,42.75,42.41a7.92,7.92,0,0,0,7.83-.59l25-21.3a8,8,0,0,1,7.59-.69l47.16,21.13A8,8,0,0,1,223.94,174.08Z"/><path fill="currentColor" d="M222.37,158.46l-47.11-21.11-.13-.06a16,16,0,0,0-15.17,1.4,8.12,8.12,0,0,0-.75.56L134.87,160c-15.42-7.49-31.34-23.29-38.83-38.51l20.78-24.71c.2-.25.39-.5.57-.77a16,16,0,0,0,1.32-15.06l0-.12L97.54,33.64a16,16,0,0,0-16.62-9.52A56.26,56.26,0,0,0,32,80c0,79.4,64.6,144,144,144a56.26,56.26,0,0,0,55.88-48.92A16,16,0,0,0,222.37,158.46ZM176,208A128.14,128.14,0,0,1,48,80,40.2,40.2,0,0,1,82.87,40a.61.61,0,0,0,0,.12l21,47L83.2,111.86a6.13,6.13,0,0,0-.57.77,16,16,0,0,0-1,15.7c9.06,18.53,27.73,37.06,46.46,46.11a16,16,0,0,0,15.75-1.14,8.44,8.44,0,0,0,.74-.56L168.89,152l47,21.05h0s.08,0,.11,0A40.21,40.21,0,0,1,176,208Z"/></g>"#;

/// A speech bubble: messages (the header's button in base.html, the welcome page's in the row with
/// its search in home.html, and the button on a listing's page that writes to its seller).
/// Phosphor's `chat-circle`.
pub const CHAT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128A96,96,0,0,1,79.93,211.11h0L42.54,223.58a8,8,0,0,1-10.12-10.12l12.47-37.39h0A96,96,0,1,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,0,0,36.18,176.88L24.83,210.93a16,16,0,0,0,20.24,20.24l34.05-11.35A104,104,0,1,0,128,24Zm0,192a87.87,87.87,0,0,1-44.06-11.81,8,8,0,0,0-6.54-.67L40,216,52.47,178.6a8,8,0,0,0-.66-6.54A88,88,0,1,1,128,216Z"/></g>"#;

/// WhatsApp's mark as a line drawing: a round bubble, its tail at the lower left, and a handset
/// filled in it; the button beside a seller's revealed number that opens a chat with it
/// (phone_stream.html).
/// Phosphor's `whatsapp-logo`.
pub const WHATSAPP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,32A96,96,0,0,0,44.89,176.07L32.42,213.46a8,8,0,0,0,10.12,10.12l37.39-12.47A96,96,0,1,0,128,32Zm24,152a80,80,0,0,1-80-80,32,32,0,0,1,32-32l16,32-12.32,18.47a48.19,48.19,0,0,0,25.85,25.85L152,136l32,16A32,32,0,0,1,152,184Z"/><path fill="currentColor" d="M187.58,144.84l-32-16a8,8,0,0,0-8,.5l-14.69,9.8a40.55,40.55,0,0,1-16-16l9.8-14.69a8,8,0,0,0,.5-8l-16-32A8,8,0,0,0,104,64a40,40,0,0,0-40,40,88.1,88.1,0,0,0,88,88,40,40,0,0,0,40-40A8,8,0,0,0,187.58,144.84ZM152,176a72.08,72.08,0,0,1-72-72A24,24,0,0,1,99.29,80.46l11.48,23L101,118a8,8,0,0,0-.73,7.51,56.47,56.47,0,0,0,30.15,30.15A8,8,0,0,0,138,155l14.62-9.74,23,11.48A24,24,0,0,1,152,176ZM128,24A104,104,0,0,0,36.18,176.88L24.83,210.93a16,16,0,0,0,20.24,20.24l34.05-11.35A104,104,0,1,0,128,24Zm0,192a87.87,87.87,0,0,1-44.06-11.81,8,8,0,0,0-6.54-.67L40,216,52.47,178.6a8,8,0,0,0-.66-6.54A88,88,0,1,1,128,216Z"/></g>"#;

/// A crescent moon: the dark theme (the menu's switch, "Tema e errët", base.html).
/// Phosphor's `moon`.
pub const MOON: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M227.89,147.89A96,96,0,1,1,108.11,28.11,96.09,96.09,0,0,0,227.89,147.89Z"/><path fill="currentColor" d="M233.54,142.23a8,8,0,0,0-8-2,88.08,88.08,0,0,1-109.8-109.8,8,8,0,0,0-10-10,104.84,104.84,0,0,0-52.91,37A104,104,0,0,0,136,224a103.09,103.09,0,0,0,62.52-20.88,104.84,104.84,0,0,0,37-52.91A8,8,0,0,0,233.54,142.23ZM188.9,190.34A88,88,0,0,1,65.66,67.11a89,89,0,0,1,31.4-26A106,106,0,0,0,96,56,104.11,104.11,0,0,0,200,160a106,106,0,0,0,14.92-1.06A89,89,0,0,1,188.9,190.34Z"/></g>"#;

/// A bell: keeping a search to hear of its new listings ("Ruaj kërkimin", browse.html), and the
/// searches kept (saved.html, saved_searches.html).
/// Phosphor's `bell`.
pub const BELL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,192H48a8,8,0,0,1-6.88-12C47.71,168.6,56,139.81,56,104a72,72,0,0,1,144,0c0,35.82,8.3,64.6,14.9,76A8,8,0,0,1,208,192Z"/><path fill="currentColor" d="M221.8,175.94C216.25,166.38,208,139.33,208,104a80,80,0,1,0-160,0c0,35.34-8.26,62.38-13.81,71.94A16,16,0,0,0,48,200H88.81a40,40,0,0,0,78.38,0H208a16,16,0,0,0,13.8-24.06ZM128,216a24,24,0,0,1-22.62-16h45.24A24,24,0,0,1,128,216ZM48,184c7.7-13.24,16-43.92,16-80a64,64,0,1,1,128,0c0,36.05,8.28,66.73,16,80Z"/></g>"#;

/// A megaphone, its mouth to the right, and the handle under it: a listing promoted
/// (notifications.rs).
/// Phosphor's `megaphone`.
pub const MEGAPHONE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,120a40,40,0,0,1-40,40H160V80h40A40,40,0,0,1,240,120Z"/><path fill="currentColor" d="M248,120a48.05,48.05,0,0,0-48-48H160.2c-2.91-.17-53.62-3.74-101.91-44.24A16,16,0,0,0,32,40V200a16,16,0,0,0,26.29,12.25c37.77-31.68,77-40.76,93.71-43.3v31.72A16,16,0,0,0,159.12,214l11,7.33A16,16,0,0,0,194.5,212l11.77-44.36A48.07,48.07,0,0,0,248,120ZM48,199.93V40h0c42.81,35.91,86.63,45,104,47.24v65.48C134.65,155,90.84,164.07,48,199.93Zm131,8,0,.11-11-7.33V168h21.6ZM200,152H168V88h32a32,32,0,1,1,0,64Z"/></g>"#;

/// A storefront: its awning, scalloped, over its walls and door; an invitation into a store
/// (notifications.rs).
/// Phosphor's `storefront`.
pub const STORE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,96v16a32,32,0,0,1-64,0V96H96v16a32,32,0,0,1-64,0V96L46.34,45.8A8,8,0,0,1,54,40H202a8,8,0,0,1,7.69,5.8Z"/><path fill="currentColor" d="M231.69,93.81,217.35,43.6A16.07,16.07,0,0,0,202,32H54A16.07,16.07,0,0,0,38.65,43.6L24.31,93.81A7.94,7.94,0,0,0,24,96v16a40,40,0,0,0,16,32v72a8,8,0,0,0,8,8H208a8,8,0,0,0,8-8V144a40,40,0,0,0,16-32V96A7.94,7.94,0,0,0,231.69,93.81ZM54,48H202l11.42,40H42.61Zm98,56v8a24,24,0,0,1-48,0v-8ZM51.06,132.2A24,24,0,0,1,40,112v-8H88v8a24,24,0,0,1-35.12,21.26A7.88,7.88,0,0,0,51.06,132.2ZM200,208H56V151.2a40.57,40.57,0,0,0,8,.8,40,40,0,0,0,32-16,40,40,0,0,0,64,0,40,40,0,0,0,32,16,40.57,40.57,0,0,0,8-.8Zm16-96a24,24,0,0,1-11.07,20.2,8.08,8.08,0,0,0-1.8,1.05A24,24,0,0,1,168,112v-8h48Z"/></g>"#;

/// A paper plane: sending a message (conversation.html).
/// Phosphor's `paper-plane-right`.
pub const SEND: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M227.91,134.86,59.93,231a8,8,0,0,1-11.44-9.67L80,128,48.49,34.72a8,8,0,0,1,11.44-9.67l168,95.85A8,8,0,0,1,227.91,134.86Z"/><path fill="currentColor" d="M231.87,114l-168-95.89A16,16,0,0,0,40.92,37.34L71.55,128,40.92,218.67A16,16,0,0,0,56,240a16.15,16.15,0,0,0,7.93-2.1l167.92-96.05a16,16,0,0,0,.05-27.89ZM56,224a.56.56,0,0,0,0-.12L85.74,136H144a8,8,0,0,0,0-16H85.74L56.06,32.16A.46.46,0,0,0,56,32l168,95.83Z"/></g>"#;

// ---------------------------------------------------------------- the redesign's

// Drawn as the "Calmer Pluribus" canvas has them, before the pages that take them are redrawn:
// each is expected to be unused until then, and the compiler says when that stops being so (an
// unfulfilled expectation), for its mark to come off as its page takes it.

/// Three lines: the menu (the header's "Menyja" button, which opens the drawer, base.html).
/// Phosphor's `list`.
pub const MENU: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,64V192H40V64Z"/><path fill="currentColor" d="M224,128a8,8,0,0,1-8,8H40a8,8,0,0,1,0-16H216A8,8,0,0,1,224,128ZM40,72H216a8,8,0,0,0,0-16H40a8,8,0,0,0,0,16ZM216,184H40a8,8,0,0,0,0,16H216a8,8,0,0,0,0-16Z"/></g>"#;

/// A bookmark: keeping a search ("Ruaj kërkimin", its mark in the accent), and the searches kept
/// (the menu's "Kërkimet e ruajtura", base.html; saved_searches.html), where the bell is now; the
/// bell stays for the notifications.
/// Phosphor's `bookmark-simple`.
pub const BOOKMARK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,48V224l-64-40L64,224V48a8,8,0,0,1,8-8H184A8,8,0,0,1,192,48Z"/><path fill="currentColor" d="M184,32H72A16,16,0,0,0,56,48V224a8,8,0,0,0,12.24,6.78L128,193.43l59.77,37.35A8,8,0,0,0,200,224V48A16,16,0,0,0,184,32Zm0,177.57-51.77-32.35a8,8,0,0,0-8.48,0L72,209.57V48H184Z"/></g>"#;

/// A globe: the language (the menu's "Gjuha", Shqip | English, base.html).
/// Phosphor's `globe`.
pub const GLOBE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24h0A104,104,0,1,0,232,128,104.12,104.12,0,0,0,128,24Zm88,104a87.61,87.61,0,0,1-3.33,24H174.16a157.44,157.44,0,0,0,0-48h38.51A87.61,87.61,0,0,1,216,128ZM102,168H154a115.11,115.11,0,0,1-26,45A115.27,115.27,0,0,1,102,168Zm-3.9-16a140.84,140.84,0,0,1,0-48h59.88a140.84,140.84,0,0,1,0,48ZM40,128a87.61,87.61,0,0,1,3.33-24H81.84a157.44,157.44,0,0,0,0,48H43.33A87.61,87.61,0,0,1,40,128ZM154,88H102a115.11,115.11,0,0,1,26-45A115.27,115.27,0,0,1,154,88Zm52.33,0H170.71a135.28,135.28,0,0,0-22.3-45.6A88.29,88.29,0,0,1,206.37,88ZM107.59,42.4A135.28,135.28,0,0,0,85.29,88H49.63A88.29,88.29,0,0,1,107.59,42.4ZM49.63,168H85.29a135.28,135.28,0,0,0,22.3,45.6A88.29,88.29,0,0,1,49.63,168Zm98.78,45.6a135.28,135.28,0,0,0,22.3-45.6h35.66A88.29,88.29,0,0,1,148.41,213.6Z"/></g>"#;

/// A big A and a small one: the text's size (the menu's "Madhësia e shkrimit", base.html).
/// Phosphor's `text-aa`.
pub const TEXT_SIZE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,164c0,15.46-14.33,28-32,28s-32-12.54-32-28,14.33-28,32-28S232,148.54,232,164ZM34.82,152h90.36L80,56Z"/><path fill="currentColor" d="M87.24,52.59a8,8,0,0,0-14.48,0l-64,136a8,8,0,1,0,14.48,6.81L39.9,160h80.2l16.66,35.4a8,8,0,1,0,14.48-6.81ZM47.43,144,80,74.79,112.57,144ZM200,96c-12.76,0-22.73,3.47-29.63,10.32a8,8,0,0,0,11.26,11.36c3.8-3.77,10-5.68,18.37-5.68,13.23,0,24,9,24,20v3.22A42.76,42.76,0,0,0,200,128c-22.06,0-40,16.15-40,36s17.94,36,40,36a42.73,42.73,0,0,0,24-7.25,8,8,0,0,0,16-.75V132C240,112.15,222.06,96,200,96Zm0,88c-13.23,0-24-9-24-20s10.77-20,24-20,24,9,24,20S213.23,184,200,184Z"/></g>"#;

/// A frame's four corners: the page's size (the menu's "Madhësia e faqes", base.html), and, drawn
/// a size each, its four choices.
/// Phosphor's `frame-corners`.
pub const PAGE_SIZE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V200a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z"/><path fill="currentColor" d="M200,80v32a8,8,0,0,1-16,0V88H160a8,8,0,0,1,0-16h32A8,8,0,0,1,200,80ZM96,168H72V144a8,8,0,0,0-16,0v32a8,8,0,0,0,8,8H96a8,8,0,0,0,0-16ZM232,56V200a16,16,0,0,1-16,16H40a16,16,0,0,1-16-16V56A16,16,0,0,1,40,40H216A16,16,0,0,1,232,56ZM216,200V56H40V200H216Z"/></g>"#;

/// A door's frame and an arrow out of it: signing out ("Dil nga llogaria", in "Njoftimet e
/// mia" and "Cilësimet").
/// Phosphor's `sign-out`.
pub const LOG_OUT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56V200a16,16,0,0,1-16,16H48V40H208A16,16,0,0,1,224,56Z"/><path fill="currentColor" d="M120,216a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V40a8,8,0,0,1,8-8h64a8,8,0,0,1,0,16H56V208h56A8,8,0,0,1,120,216Zm109.66-93.66-40-40a8,8,0,0,0-11.32,11.32L204.69,120H112a8,8,0,0,0,0,16h92.69l-26.35,26.34a8,8,0,0,0,11.32,11.32l40-40A8,8,0,0,0,229.66,122.34Z"/></g>"#;

/// An i in a circle: a note on how things stand (a seller's own listing awaiting review,
/// expired or paused), and the page that something went wrong on.
/// Phosphor's `info`.
#[expect(dead_code, reason = "the notices and the error page take it when they are redrawn")]
pub const INFO: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M144,176a8,8,0,0,1-8,8,16,16,0,0,1-16-16V128a8,8,0,0,1,0-16,16,16,0,0,1,16,16v40A8,8,0,0,1,144,176Zm88-48A104,104,0,1,1,128,24,104.11,104.11,0,0,1,232,128Zm-16,0a88,88,0,1,0-88,88A88.1,88.1,0,0,0,216,128ZM124,96a12,12,0,1,0-12-12A12,12,0,0,0,124,96Z"/></g>"#;

/// A crosshair: finding where the visitor is (the maps' "Vendndodhja ime": map.html, the browse
/// row's map card).
/// Phosphor's `crosshair`.
pub const LOCATE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M160,128a32,32,0,1,1-32-32A32,32,0,0,1,160,128Z"/><path fill="currentColor" d="M232,120h-8.34A96.14,96.14,0,0,0,136,32.34V24a8,8,0,0,0-16,0v8.34A96.14,96.14,0,0,0,32.34,120H24a8,8,0,0,0,0,16h8.34A96.14,96.14,0,0,0,120,223.66V232a8,8,0,0,0,16,0v-8.34A96.14,96.14,0,0,0,223.66,136H232a8,8,0,0,0,0-16Zm-96,87.6V200a8,8,0,0,0-16,0v7.6A80.15,80.15,0,0,1,48.4,136H56a8,8,0,0,0,0-16H48.4A80.15,80.15,0,0,1,120,48.4V56a8,8,0,0,0,16,0V48.4A80.15,80.15,0,0,1,207.6,120H200a8,8,0,0,0,0,16h7.6A80.15,80.15,0,0,1,136,207.6ZM128,88a40,40,0,1,0,40,40A40,40,0,0,0,128,88Zm0,64a24,24,0,1,1,24-24A24,24,0,0,1,128,152Z"/></g>"#;

/// Two squares, each with its lines of words: the user's own listings (the menu's "Njoftimet e
/// mia", base.html), as the list view's icon has them.
/// Phosphor's `cards`.
pub const LISTINGS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,88V200a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8V88a8,8,0,0,1,8-8H184A8,8,0,0,1,192,88Z"/><path fill="currentColor" d="M184,72H40A16,16,0,0,0,24,88V200a16,16,0,0,0,16,16H184a16,16,0,0,0,16-16V88A16,16,0,0,0,184,72Zm0,128H40V88H184V200ZM232,56V176a8,8,0,0,1-16,0V56H64a8,8,0,0,1,0-16H216A16,16,0,0,1,232,56Z"/></g>"#;

/// Three sliders: the settings (the menu's "Cilësimet", base.html), as the filters' button has them.
/// Phosphor's `sliders-horizontal`.
pub const SETTINGS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,80a24,24,0,1,1-24-24A24,24,0,0,1,128,80Zm40,72a24,24,0,1,0,24,24A24,24,0,0,0,168,152Z"/><path fill="currentColor" d="M40,88H73a32,32,0,0,0,62,0h81a8,8,0,0,0,0-16H135a32,32,0,0,0-62,0H40a8,8,0,0,0,0,16Zm64-24A16,16,0,1,1,88,80,16,16,0,0,1,104,64ZM216,168H199a32,32,0,0,0-62,0H40a8,8,0,0,0,0,16h97a32,32,0,0,0,62,0h17a8,8,0,0,0,0-16Zm-48,24a16,16,0,1,1,16-16A16,16,0,0,1,168,192Z"/></g>"#;

/// A cross: closing what floats over the page (the menu's "Mbyll menynë", base.html).
/// Phosphor's `x`.
pub const CLOSE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,56V200a16,16,0,0,1-16,16H56a16,16,0,0,1-16-16V56A16,16,0,0,1,56,40H200A16,16,0,0,1,216,56Z"/><path fill="currentColor" d="M205.66,194.34a8,8,0,0,1-11.32,11.32L128,139.31,61.66,205.66a8,8,0,0,1-11.32-11.32L116.69,128,50.34,61.66A8,8,0,0,1,61.66,50.34L128,116.69l66.34-66.35a8,8,0,0,1,11.32,11.32L139.31,128Z"/></g>"#;

/// A plus: adding a listing (the menu's "Posto njoftim" and the rail's square, base.html).
/// Phosphor's `plus`.
pub const PLUS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,56V200a16,16,0,0,1-16,16H56a16,16,0,0,1-16-16V56A16,16,0,0,1,56,40H200A16,16,0,0,1,216,56Z"/><path fill="currentColor" d="M224,128a8,8,0,0,1-8,8H136v80a8,8,0,0,1-16,0V136H40a8,8,0,0,1,0-16h80V40a8,8,0,0,1,16,0v80h80A8,8,0,0,1,224,128Z"/></g>"#;

/// A house, its door at its foot: the front page (the rail's "Kreu", base.html).
/// Phosphor's `house-simple`.
pub const HOME: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,120v96H40V120a8,8,0,0,1,2.34-5.66l80-80a8,8,0,0,1,11.32,0l80,80A8,8,0,0,1,216,120Z"/><path fill="currentColor" d="M219.31,108.68l-80-80a16,16,0,0,0-22.62,0l-80,80A15.87,15.87,0,0,0,32,120v96a8,8,0,0,0,8,8H216a8,8,0,0,0,8-8V120A15.87,15.87,0,0,0,219.31,108.68ZM208,208H48V120l80-80,80,80Z"/></g>"#;

/// An office block with a lower wing: a store without a logo of its own, in its round avatar (a
/// listing's seller row, its page, an invitation into it: seller_card.html, profile.html,
/// store_invitation.html), as a person has the initial of their name.
/// Phosphor's `buildings`.
pub const BUSINESS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M136,32V216H40V85.35a8,8,0,0,1,3.56-6.66l80-53.33A8,8,0,0,1,136,32Z"/><path fill="currentColor" d="M240,208H224V96a16,16,0,0,0-16-16H144V32a16,16,0,0,0-24.88-13.32L39.12,72A16,16,0,0,0,32,85.34V208H16a8,8,0,0,0,0,16H240a8,8,0,0,0,0-16ZM208,96V208H144V96ZM48,85.34,128,32V208H48ZM112,112v16a8,8,0,0,1-16,0V112a8,8,0,1,1,16,0Zm-32,0v16a8,8,0,0,1-16,0V112a8,8,0,1,1,16,0Zm0,56v16a8,8,0,0,1-16,0V168a8,8,0,0,1,16,0Zm32,0v16a8,8,0,0,1-16,0V168a8,8,0,0,1,16,0Z"/></g>"#;

/// A tick: a search kept ("Kërkimi u ruajt", browse.html).
/// Phosphor's `check`.
pub const CHECK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,56V200a16,16,0,0,1-16,16H40a16,16,0,0,1-16-16V56A16,16,0,0,1,40,40H216A16,16,0,0,1,232,56Z"/><path fill="currentColor" d="M205.66,85.66l-96,96a8,8,0,0,1-11.32,0l-40-40a8,8,0,0,1,11.32-11.32L104,164.69l90.34-90.35a8,8,0,0,1,11.32,11.32Z"/></g>"#;

/// A chevron, down: what opens under itself (the row's filter dropdowns, filter_drop.html).
/// Phosphor's `caret-down`.
pub const CHEVRON_DOWN: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,96l-80,80L48,96Z"/><path fill="currentColor" d="M215.39,92.94A8,8,0,0,0,208,88H48a8,8,0,0,0-5.66,13.66l80,80a8,8,0,0,0,11.32,0l80-80A8,8,0,0,0,215.39,92.94ZM128,164.69,67.31,104H188.69Z"/></g>"#;

/// A magnifier: searching (the top bar's search and the welcome page's, base.html, home.html; a search's suggestions; nothing found).
/// Phosphor's `magnifying-glass`.
pub const SEARCH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,112a80,80,0,1,1-80-80A80,80,0,0,1,192,112Z"/><path fill="currentColor" d="M229.66,218.34,179.6,168.28a88.21,88.21,0,1,0-11.32,11.31l50.06,50.07a8,8,0,0,0,11.32-11.32ZM40,112a72,72,0,1,1,72,72A72.08,72.08,0,0,1,40,112Z"/></g>"#;

/// A person: the account (the top bar's button, base.html).
/// Phosphor's `user`.
pub const USER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,96a64,64,0,1,1-64-64A64,64,0,0,1,192,96Z"/><path fill="currentColor" d="M230.92,212c-15.23-26.33-38.7-45.21-66.09-54.16a72,72,0,1,0-73.66,0C63.78,166.78,40.31,185.66,25.08,212a8,8,0,1,0,13.85,8c18.84-32.56,52.14-52,89.07-52s70.23,19.44,89.07,52a8,8,0,1,0,13.85-8ZM72,96a56,56,0,1,1,56,56A56.06,56.06,0,0,1,72,96Z"/></g>"#;

/// A chevron, left: back, and the photo or listing before.
/// Phosphor's `caret-left`.
pub const CHEVRON_LEFT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M160,48V208L80,128Z"/><path fill="currentColor" d="M163.06,40.61a8,8,0,0,0-8.72,1.73l-80,80a8,8,0,0,0,0,11.32l80,80A8,8,0,0,0,168,208V48A8,8,0,0,0,163.06,40.61ZM152,188.69,91.31,128,152,67.31Z"/></g>"#;

/// A chevron, right: on, the photo or listing after, a row that leads somewhere.
/// Phosphor's `caret-right`.
pub const CHEVRON_RIGHT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,128,96,208V48Z"/><path fill="currentColor" d="M181.66,122.34l-80-80A8,8,0,0,0,88,48V208a8,8,0,0,0,13.66,5.66l80-80A8,8,0,0,0,181.66,122.34ZM104,188.69V67.31L164.69,128Z"/></g>"#;

/// A line going up: the most looked at ("Më të klikuarat", browse.html).
/// Phosphor's `trend-up`.
pub const TREND_UP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,56v64L168,56Z"/><path fill="currentColor" d="M232,48H168a8,8,0,0,0-5.66,13.66L188.69,88,136,140.69l-34.34-34.35a8,8,0,0,0-11.32,0l-72,72a8,8,0,0,0,11.32,11.32L96,123.31l34.34,34.35a8,8,0,0,0,11.32,0L200,99.31l26.34,26.35A8,8,0,0,0,240,120V56A8,8,0,0,0,232,48Zm-8,52.69L187.31,64H224Z"/></g>"#;

/// A calculator: the loan's ("Llogarit kredinë", listing.html).
/// Phosphor's `calculator`.
pub const CALCULATOR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M176,64v48H80V64Z"/><path fill="currentColor" d="M80,120h96a8,8,0,0,0,8-8V64a8,8,0,0,0-8-8H80a8,8,0,0,0-8,8v48A8,8,0,0,0,80,120Zm8-48h80v32H88ZM200,24H56A16,16,0,0,0,40,40V216a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V40A16,16,0,0,0,200,24Zm0,192H56V40H200ZM100,148a12,12,0,1,1-12-12A12,12,0,0,1,100,148Zm40,0a12,12,0,1,1-12-12A12,12,0,0,1,140,148Zm40,0a12,12,0,1,1-12-12A12,12,0,0,1,180,148Zm-80,40a12,12,0,1,1-12-12A12,12,0,0,1,100,188Zm40,0a12,12,0,1,1-12-12A12,12,0,0,1,140,188Zm40,0a12,12,0,1,1-12-12A12,12,0,0,1,180,188Z"/></g>"#;

/// A chain's links: sharing a listing's address (listing_actions.html, share_note.html).
/// Phosphor's `link`.
pub const LINK: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M218.34,119.6,183.6,154.34a46.58,46.58,0,0,1-44.31,12.26c-.31.34-.62.67-.95,1L103.6,202.34A46.63,46.63,0,1,1,37.66,136.4L72.4,101.66A46.6,46.6,0,0,1,116.71,89.4c.31-.34.62-.67,1-1L152.4,53.66a46.63,46.63,0,0,1,65.94,65.94Z"/><path fill="currentColor" d="M240,88.23a54.43,54.43,0,0,1-16,37L189.25,160a54.27,54.27,0,0,1-38.63,16h-.05A54.63,54.63,0,0,1,96,119.84a8,8,0,0,1,16,.45A38.62,38.62,0,0,0,150.58,160h0a38.39,38.39,0,0,0,27.31-11.31l34.75-34.75a38.63,38.63,0,0,0-54.63-54.63l-11,11A8,8,0,0,1,135.7,59l11-11A54.65,54.65,0,0,1,224,48,54.86,54.86,0,0,1,240,88.23ZM109,185.66l-11,11A38.41,38.41,0,0,1,70.6,208h0a38.63,38.63,0,0,1-27.29-65.94L78,107.31A38.63,38.63,0,0,1,144,135.71a8,8,0,0,0,7.78,8.22H152a8,8,0,0,0,8-7.78A54.86,54.86,0,0,0,144,96a54.65,54.65,0,0,0-77.27,0L32,130.75A54.62,54.62,0,0,0,70.56,224h0a54.28,54.28,0,0,0,38.64-16l11-11A8,8,0,0,0,109,185.66Z"/></g>"#;

/// Two sheets: copying a number or an address (phone_stream.html, store_people.html).
/// Phosphor's `copy`.
pub const COPY: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,40V168H168V88H88V40Z"/><path fill="currentColor" d="M216,32H88a8,8,0,0,0-8,8V80H40a8,8,0,0,0-8,8V216a8,8,0,0,0,8,8H168a8,8,0,0,0,8-8V176h40a8,8,0,0,0,8-8V40A8,8,0,0,0,216,32ZM160,208H48V96H160Zm48-48H176V88a8,8,0,0,0-8-8H96V48H208Z"/></g>"#;

/// An arrow out of a square: what a square metre costs (area_price.html).
/// Phosphor's `arrow-square-up-right`.
pub const AREA_PRICE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,48V208a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8H208A8,8,0,0,1,216,48Z"/><path fill="currentColor" d="M208,32H48A16,16,0,0,0,32,48V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Zm0,176H48V48H208ZM90.34,165.66a8,8,0,0,1,0-11.32L140.69,104H112a8,8,0,0,1,0-16h48a8,8,0,0,1,8,8v48a8,8,0,0,1-16,0V115.31l-50.34,50.35a8,8,0,0,1-11.32,0Z"/></g>"#;

/// Four squares: the cards small, four to a line (browse.rs, Cards::Small).
/// Phosphor's `squares-four`.
pub const VIEW_GRID: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M112,56v48a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8h48A8,8,0,0,1,112,56Zm88-8H152a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V56A8,8,0,0,0,200,48Zm-96,96H56a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V152A8,8,0,0,0,104,144Zm96,0H152a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V152A8,8,0,0,0,200,144Z"/><path fill="currentColor" d="M200,136H152a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V152A16,16,0,0,0,200,136Zm0,64H152V152h48v48ZM104,40H56A16,16,0,0,0,40,56v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V56A16,16,0,0,0,104,40Zm0,64H56V56h48v48Zm96-64H152a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V56A16,16,0,0,0,200,40Zm0,64H152V56h48v48Zm-96,32H56a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V152A16,16,0,0,0,104,136Zm0,64H56V152h48v48Z"/></g>"#;

/// A square: the cards big, one or two to a line (browse.rs, Cards::Big).
/// Phosphor's `square`.
pub const VIEW_BIG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,48V208a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8H208A8,8,0,0,1,216,48Z"/><path fill="currentColor" d="M208,32H48A16,16,0,0,0,32,48V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Zm0,176H48V48H208V208Z"/></g>"#;

/// Lines with their dots: the cards as a list (browse.rs, Cards::List).
/// Phosphor's `list-bullets`.
pub const VIEW_LIST: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,64V192H88V64Z"/><path fill="currentColor" d="M80,64a8,8,0,0,1,8-8H216a8,8,0,0,1,0,16H88A8,8,0,0,1,80,64Zm136,56H88a8,8,0,1,0,0,16H216a8,8,0,0,0,0-16Zm0,64H88a8,8,0,1,0,0,16H216a8,8,0,0,0,0-16ZM44,52A12,12,0,1,0,56,64,12,12,0,0,0,44,52Zm0,64a12,12,0,1,0,12,12A12,12,0,0,0,44,116Zm0,64a12,12,0,1,0,12,12A12,12,0,0,0,44,180Z"/></g>"#;

/// An envelope: a seller's e-mail address, for one who signed in by e-mail (mail.svg, contact_bar.html).
/// Phosphor's `envelope-simple`.
pub const MAIL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56l-96,88L32,56Z"/><path fill="currentColor" d="M224,48H32a8,8,0,0,0-8,8V192a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A8,8,0,0,0,224,48ZM203.43,64,128,133.15,52.57,64ZM216,192H40V74.19l82.59,75.71a8,8,0,0,0,10.82,0L216,74.19V192Z"/></g>"#;

/// Other things, and a category without an icon of its own yet: a box.
/// Phosphor's `package`.
const BOX: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M128,129.09V232a8,8,0,0,1-3.84-1l-88-48.18a8,8,0,0,1-4.16-7V80.18a8,8,0,0,1,.7-3.25Z"/><path fill="currentColor" d="M223.68,66.15,135.68,18a15.88,15.88,0,0,0-15.36,0l-88,48.17a16,16,0,0,0-8.32,14v95.64a16,16,0,0,0,8.32,14l88,48.17a15.88,15.88,0,0,0,15.36,0l88-48.17a16,16,0,0,0,8.32-14V80.18A16,16,0,0,0,223.68,66.15ZM128,32l80.34,44-29.77,16.3-80.35-44ZM128,120,47.66,76l33.9-18.56,80.34,44ZM40,90l80,43.78v85.79L40,175.82Zm176,85.78h0l-80,43.79V133.82l32-17.51V152a8,8,0,0,0,16,0V107.55L216,90v85.77Z"/></g>"#;

/// A car, from the side.
/// Phosphor's `car`.
const CAR: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M184,176h40v24a8,8,0,0,1-8,8H192a8,8,0,0,1-8-8ZM32,200a8,8,0,0,0,8,8H64a8,8,0,0,0,8-8V176H32ZM194.11,44.75A8,8,0,0,0,186.8,40H69.2a8,8,0,0,0-7.31,4.75L32,112H224Z"/><path fill="currentColor" d="M240,104H229.2L201.42,41.5A16,16,0,0,0,186.8,32H69.2a16,16,0,0,0-14.62,9.5L26.8,104H16a8,8,0,0,0,0,16h8v80a16,16,0,0,0,16,16H64a16,16,0,0,0,16-16V184h96v16a16,16,0,0,0,16,16h24a16,16,0,0,0,16-16V120h8a8,8,0,0,0,0-16ZM69.2,48H186.8l24.89,56H44.31ZM64,200H40V184H64Zm128,0V184h24v16Zm24-32H40V120H216ZM56,144a8,8,0,0,1,8-8H80a8,8,0,0,1,0,16H64A8,8,0,0,1,56,144Zm112,0a8,8,0,0,1,8-8h16a8,8,0,0,1,0,16H176A8,8,0,0,1,168,144Z"/></g>"#;

/// A phone.
/// Phosphor's `device-mobile`.
const HANDSET: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192,56V200H64V56Z"/><path fill="currentColor" d="M176,16H80A24,24,0,0,0,56,40V216a24,24,0,0,0,24,24h96a24,24,0,0,0,24-24V40A24,24,0,0,0,176,16ZM72,64H184V192H72Zm8-32h96a8,8,0,0,1,8,8v8H72V40A8,8,0,0,1,80,32Zm96,192H80a8,8,0,0,1-8-8v-8H184v8A8,8,0,0,1,176,224Z"/></g>"#;

/// A briefcase.
/// Phosphor's `briefcase`.
const BRIEFCASE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,118.31V200a8,8,0,0,1-8,8H40a8,8,0,0,1-8-8V118.31h0A191.14,191.14,0,0,0,128,144,191.08,191.08,0,0,0,224,118.31Z"/><path fill="currentColor" d="M104,112a8,8,0,0,1,8-8h32a8,8,0,0,1,0,16H112A8,8,0,0,1,104,112ZM232,72V200a16,16,0,0,1-16,16H40a16,16,0,0,1-16-16V72A16,16,0,0,1,40,56H80V48a24,24,0,0,1,24-24h48a24,24,0,0,1,24,24v8h40A16,16,0,0,1,232,72ZM96,56h64V48a8,8,0,0,0-8-8H104a8,8,0,0,0-8,8ZM40,72v41.62A184.07,184.07,0,0,0,128,136a184,184,0,0,0,88-22.39V72ZM216,200V131.63A200.25,200.25,0,0,1,128,152a200.19,200.19,0,0,1-88-20.36V200H216Z"/></g>"#;

/// A hammer.
/// Phosphor's `hammer`.
const HAMMER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M245.66,129,217,157.66a8,8,0,0,1-11.3,0L168.1,120l-28,28-32-32,28-28L80,31.78a87.81,87.81,0,0,1,98.31,18l67.35,67.89A8,8,0,0,1,245.66,129Z"/><path fill="currentColor" d="M251.34,112,183.88,44.08a96.1,96.1,0,0,0-135.77,0l-.09.09L34.25,58.4A8,8,0,0,0,45.74,69.53L59.47,55.35a79.92,79.92,0,0,1,18.71-13.9L124.68,88l-96,96a16,16,0,0,0,0,22.63l20.69,20.69a16,16,0,0,0,22.63,0l96-96,14.34,14.34h0L200,163.3a16,16,0,0,0,22.63,0l28.69-28.69A16,16,0,0,0,251.34,112ZM60.68,216,40,195.31l68-68L128.68,148ZM162.34,114.32,140,136.67,119.31,116l22.35-22.35a8,8,0,0,0,0-11.32L94.32,35a80,80,0,0,1,78.23,20.41l44.22,44.51L188,128.66l-14.34-14.34A8,8,0,0,0,162.34,114.32Zm49,37.66-12-12L228,111.25l12,12Z"/></g>"#;

/// A T-shirt.
/// Phosphor's `t-shirt`.
const T_SHIRT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M247.11,78.77l-19.27,36.81a8.44,8.44,0,0,1-7.5,4.42H192V40l51.78,28.25A7.81,7.81,0,0,1,247.11,78.77Zm-238.22,0,19.27,36.81a8.44,8.44,0,0,0,7.5,4.42H64V40L12.22,68.25A7.81,7.81,0,0,0,8.89,78.77Z"/><path fill="currentColor" d="M247.59,61.22,195.83,33A8,8,0,0,0,192,32H160a8,8,0,0,0-8,8,24,24,0,0,1-48,0,8,8,0,0,0-8-8H64a8,8,0,0,0-3.84,1L8.41,61.22A15.76,15.76,0,0,0,1.82,82.48l19.27,36.81A16.37,16.37,0,0,0,35.67,128H56v80a16,16,0,0,0,16,16H184a16,16,0,0,0,16-16V128h20.34a16.37,16.37,0,0,0,14.58-8.71l19.27-36.81A15.76,15.76,0,0,0,247.59,61.22ZM35.67,112a.62.62,0,0,1-.41-.13L16.09,75.26,56,53.48V112ZM184,208H72V48h16.8a40,40,0,0,0,78.38,0H184Zm36.75-96.14a.55.55,0,0,1-.41.14H200V53.48l39.92,21.78Z"/></g>"#;

/// A tennis ball.
/// Phosphor's `barbell`.
const TENNIS_BALL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M96,64V192a8,8,0,0,1-8,8H64a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H88A8,8,0,0,1,96,64Zm96-8H168a8,8,0,0,0-8,8V192a8,8,0,0,0,8,8h24a8,8,0,0,0,8-8V64A8,8,0,0,0,192,56Z"/><path fill="currentColor" d="M248,120h-8V88a16,16,0,0,0-16-16H208V64a16,16,0,0,0-16-16H168a16,16,0,0,0-16,16v56H104V64A16,16,0,0,0,88,48H64A16,16,0,0,0,48,64v8H32A16,16,0,0,0,16,88v32H8a8,8,0,0,0,0,16h8v32a16,16,0,0,0,16,16H48v8a16,16,0,0,0,16,16H88a16,16,0,0,0,16-16V136h48v56a16,16,0,0,0,16,16h24a16,16,0,0,0,16-16v-8h16a16,16,0,0,0,16-16V136h8a8,8,0,0,0,0-16ZM32,168V88H48v80Zm56,24H64V64H88V192Zm104,0H168V64h24V175.83c0,.06,0,.11,0,.17s0,.12,0,.17V192Zm32-24H208V88h16Z"/></g>"#;

/// A factory.
/// Phosphor's `factory`.
const FACTORY: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,136v80H40V88l64,48V88l64,48Z"/><path fill="currentColor" d="M116,176a8,8,0,0,1-8,8H80a8,8,0,0,1,0-16h28A8,8,0,0,1,116,176Zm60-8H148a8,8,0,0,0,0,16h28a8,8,0,0,0,0-16Zm64,48a8,8,0,0,1-8,8H24a8,8,0,0,1,0-16h8V88a8,8,0,0,1,12.8-6.4L96,120V88a8,8,0,0,1,12.8-6.4l38.74,29.05L159.1,29.74A16.08,16.08,0,0,1,174.94,16h18.12A16.08,16.08,0,0,1,208.9,29.74l15,105.13s.08.78.08,1.13v72h8A8,8,0,0,1,240,216Zm-77.86-94.4,8.53,6.4h36.11L193.06,32H174.94ZM48,208H208V144H168a8,8,0,0,1-4.8-1.6l-14.4-10.8,0,0L112,104v32a8,8,0,0,1-12.8,6.4L48,104Z"/></g>"#;

/// A wrench.
/// Phosphor's `wrench`.
const WRENCH: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,96a64,64,0,0,1-94.94,56L73,217A24,24,0,0,1,39,183L104,126.94a64,64,0,0,1,80-90.29L144,80l5.66,26.34L176,112l43.35-40A63.8,63.8,0,0,1,224,96Z"/><path fill="currentColor" d="M226.76,69a8,8,0,0,0-12.84-2.88l-40.3,37.19-17.23-3.7-3.7-17.23,37.19-40.3A8,8,0,0,0,187,29.24,72,72,0,0,0,88,96,72.34,72.34,0,0,0,94,124.94L33.79,177c-.15.12-.29.26-.43.39a32,32,0,0,0,45.26,45.26c.13-.13.27-.28.39-.42L131.06,162A72,72,0,0,0,232,96,71.56,71.56,0,0,0,226.76,69ZM160,152a56.14,56.14,0,0,1-27.07-7,8,8,0,0,0-9.92,1.77L67.11,211.51a16,16,0,0,1-22.62-22.62L109.18,133a8,8,0,0,0,1.77-9.93,56,56,0,0,1,58.36-82.31l-31.2,33.81a8,8,0,0,0-1.94,7.1L141.83,108a8,8,0,0,0,6.14,6.14l26.35,5.66a8,8,0,0,0,7.1-1.94l33.81-31.2A56.06,56.06,0,0,1,160,152Z"/></g>"#;

/// A nut.
/// Phosphor's `nut`.
const NUT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M219.84,73.16,131.84,25a8,8,0,0,0-7.68,0l-88,48.18a8,8,0,0,0-4.16,7v95.64a8,8,0,0,0,4.16,7l88,48.18a8,8,0,0,0,7.68,0l88-48.18a8,8,0,0,0,4.16-7V80.18A8,8,0,0,0,219.84,73.16ZM128,168a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z"/><path fill="currentColor" d="M128,80a48,48,0,1,0,48,48A48.06,48.06,0,0,0,128,80Zm0,80a32,32,0,1,1,32-32A32,32,0,0,1,128,160Zm95.68-93.85L135.68,18a15.88,15.88,0,0,0-15.36,0l-88,48.17a16,16,0,0,0-8.32,14v95.64a16,16,0,0,0,8.32,14l88,48.17a15.88,15.88,0,0,0,15.36,0l88-48.17h0a16,16,0,0,0,8.32-14V80.18A16,16,0,0,0,223.68,66.15ZM128,224,40,175.82V80.18L128,32l88,48.17v95.63Z"/></g>"#;

/// A loudspeaker.
/// Phosphor's `speaker-high`.
const LOUDSPEAKER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M80,88v80H32a8,8,0,0,1-8-8V96a8,8,0,0,1,8-8Z"/><path fill="currentColor" d="M155.51,24.81a8,8,0,0,0-8.42.88L77.25,80H32A16,16,0,0,0,16,96v64a16,16,0,0,0,16,16H77.25l69.84,54.31A8,8,0,0,0,160,224V32A8,8,0,0,0,155.51,24.81ZM32,96H72v64H32ZM144,207.64,88,164.09V91.91l56-43.55Zm54-106.08a40,40,0,0,1,0,52.88,8,8,0,0,1-12-10.58,24,24,0,0,0,0-31.72,8,8,0,0,1,12-10.58ZM248,128a79.9,79.9,0,0,1-20.37,53.34,8,8,0,0,1-11.92-10.67,64,64,0,0,0,0-85.33,8,8,0,1,1,11.92-10.67A79.83,79.83,0,0,1,248,128Z"/></g>"#;

/// A navigation arrow.
/// Phosphor's `navigation-arrow`.
const NAVIGATION_ARROW: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M234.35,129,152,152,129,234.35a8,8,0,0,1-15.21.27l-65.28-176A8,8,0,0,1,58.63,48.46l176,65.28A8,8,0,0,1,234.35,129Z"/><path fill="currentColor" d="M237.33,106.21,61.41,41l-.16-.05A16,16,0,0,0,40.9,61.25a1,1,0,0,0,.05.16l65.26,175.92A15.77,15.77,0,0,0,121.28,248h.3a15.77,15.77,0,0,0,15-11.29l.06-.2,21.84-78,78-21.84.2-.06a16,16,0,0,0,.62-30.38ZM149.84,144.3a8,8,0,0,0-5.54,5.54L121.3,232l-.06-.17L56,56l175.82,65.22.16.06Z"/></g>"#;

/// A racing flag.
/// Phosphor's `flag-checkered`.
const RACING_FLAG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,56v60c-19.89,17.23-38.23,18.39-56,13.48v-60C185.76,74.39,204.11,73.23,224,56ZM104,102.52v-60C86.24,37.61,67.89,38.77,48,56v60c19.89-17.22,38.23-18.39,56-13.48v60c21.62,6,42.38,21,64,27v-60c-21.62-6-42.38-21-64-27Z"/><path fill="currentColor" d="M227.32,48.75A8,8,0,0,0,218.76,50c-28,24.22-51.72,12.48-79.21-1.13C111.07,34.76,78.78,18.79,42.76,50A8,8,0,0,0,40,56V224a8,8,0,0,0,16,0V179.77c26.79-21.16,49.87-9.75,76.45,3.41,28.49,14.09,60.77,30.06,96.79-1.13a8,8,0,0,0,2.76-6V56A8,8,0,0,0,227.32,48.75ZM216,71.6v40.65c-14,11.06-27,13.22-40,10.88V79.34A60.05,60.05,0,0,0,216,71.6Zm-56,3.76v43c-6.66-2.67-13.43-6-20.45-9.48-8.82-4.37-18-8.91-27.55-12.18v-43c6.66,2.66,13.43,6,20.45,9.48C141.27,67.55,150.46,72.09,160,75.36ZM96,48.91V92.69a60.06,60.06,0,0,0-40,7.75V59.78C70,48.72,83,46.57,96,48.91ZM86.58,152A60.06,60.06,0,0,0,56,160.43V119.78c14-11.06,27-13.22,40-10.88v43.8A65.61,65.61,0,0,0,86.58,152ZM112,156.67v-43c6.66,2.66,13.43,6,20.45,9.48,8.82,4.37,18,8.9,27.55,12.17v43c-6.66-2.67-13.43-6-20.45-9.48C130.73,164.47,121.54,159.94,112,156.67Zm64,26.45v-43.8a65.61,65.61,0,0,0,9.42.72A60.11,60.11,0,0,0,216,131.57v40.68C202,183.31,189,185.46,176,183.12Z"/></g>"#;

/// A mat with its grooves.
/// Phosphor's `grid-four`.
const MAT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,56V200a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8H200A8,8,0,0,1,208,56Z"/><path fill="currentColor" d="M200,40H56A16,16,0,0,0,40,56V200a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V56A16,16,0,0,0,200,40Zm0,80H136V56h64ZM120,56v64H56V56ZM56,136h64v64H56Zm144,64H136V136h64v64Z"/></g>"#;

/// A trailer: its box, wheel and drawbar.
/// Phosphor's `truck-trailer`.
const TRAILER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M16,72H120V184H104a24,24,0,0,0-48,0,24,24,0,0,0-48,0V80A8,8,0,0,1,16,72Zm200,32V96a48,48,0,0,0-48-48h-8a8,8,0,0,0-8,8V184h48a24,24,0,0,1,48,0V136A32,32,0,0,0,216,104Z"/><path fill="currentColor" d="M224,96.8V96a56.06,56.06,0,0,0-56-56h-8a16,16,0,0,0-16,16V176H128V72a8,8,0,0,0-8-8H16A16,16,0,0,0,0,80V184a32,32,0,0,0,56,21.13A32,32,0,0,0,111,192h82a32,32,0,0,0,63-8V136A40.07,40.07,0,0,0,224,96.8ZM160,56h8a40,40,0,0,1,40,40v8a8,8,0,0,0,8,8,24,24,0,0,1,24,24v20.31A31.71,31.71,0,0,0,224,152a32.06,32.06,0,0,0-31,24H160ZM112,80v96h-1a32,32,0,0,0-55-13.13,31.9,31.9,0,0,0-40-6.56V80ZM32,200a16,16,0,1,1,16-16A16,16,0,0,1,32,200Zm48,0a16,16,0,1,1,16-16A16,16,0,0,1,80,200Zm144,0a16,16,0,1,1,16-16A16,16,0,0,1,224,200Z"/></g>"#;

/// Two blocks of flats.
/// Phosphor's `buildings`.
const FLATS: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M136,32V216H40V85.35a8,8,0,0,1,3.56-6.66l80-53.33A8,8,0,0,1,136,32Z"/><path fill="currentColor" d="M240,208H224V96a16,16,0,0,0-16-16H144V32a16,16,0,0,0-24.88-13.32L39.12,72A16,16,0,0,0,32,85.34V208H16a8,8,0,0,0,0,16H240a8,8,0,0,0,0-16ZM208,96V208H144V96ZM48,85.34,128,32V208H48ZM112,112v16a8,8,0,0,1-16,0V112a8,8,0,1,1,16,0Zm-32,0v16a8,8,0,0,1-16,0V112a8,8,0,1,1,16,0Zm0,56v16a8,8,0,0,1-16,0V168a8,8,0,0,1,16,0Zm32,0v16a8,8,0,0,1-16,0V168a8,8,0,0,1,16,0Z"/></g>"#;

/// A house with a chimney and a door.
/// Phosphor's `house-line`.
const VILLA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M216,116.69V216H152V152H104v64H40V116.69l82.34-82.35a8,8,0,0,1,11.32,0Z"/><path fill="currentColor" d="M240,208H224V136l2.34,2.34A8,8,0,0,0,237.66,127L139.31,28.68a16,16,0,0,0-22.62,0L18.34,127a8,8,0,0,0,11.32,11.31L32,136v72H16a8,8,0,0,0,0,16H240a8,8,0,0,0,0-16ZM48,120l80-80,80,80v88H160V152a8,8,0,0,0-8-8H104a8,8,0,0,0-8,8v56H48Zm96,88H112V160h32Z"/></g>"#;

/// A shop front under its awning.
/// Phosphor's `storefront`.
const SHOP: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M224,96v16a32,32,0,0,1-64,0V96H96v16a32,32,0,0,1-64,0V96L46.34,45.8A8,8,0,0,1,54,40H202a8,8,0,0,1,7.69,5.8Z"/><path fill="currentColor" d="M231.69,93.81,217.35,43.6A16.07,16.07,0,0,0,202,32H54A16.07,16.07,0,0,0,38.65,43.6L24.31,93.81A7.94,7.94,0,0,0,24,96v16a40,40,0,0,0,16,32v72a8,8,0,0,0,8,8H208a8,8,0,0,0,8-8V144a40,40,0,0,0,16-32V96A7.94,7.94,0,0,0,231.69,93.81ZM54,48H202l11.42,40H42.61Zm98,56v8a24,24,0,0,1-48,0v-8ZM51.06,132.2A24,24,0,0,1,40,112v-8H88v8a24,24,0,0,1-35.12,21.26A7.88,7.88,0,0,0,51.06,132.2ZM200,208H56V151.2a40.57,40.57,0,0,0,8,.8,40,40,0,0,0,32-16,40,40,0,0,0,64,0,40,40,0,0,0,32,16,40.57,40.57,0,0,0,8-.8Zm16-96a24,24,0,0,1-11.07,20.2,8.08,8.08,0,0,0-1.8,1.05A24,24,0,0,1,168,112v-8h48Z"/></g>"#;

/// A plot of land, a flag in it.
/// Phosphor's `flag-banner`.
const PLOT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M232,56l-45.71,96H40l48-48L40,56Z"/><path fill="currentColor" d="M238.76,51.73A8,8,0,0,0,232,48H40a8,8,0,0,0-5.66,13.66L76.69,104,34.34,146.34A8,8,0,0,0,40,160H173.62l-28.84,60.56a8,8,0,1,0,14.44,6.88l80-168A8,8,0,0,0,238.76,51.73ZM181.23,144H59.31l34.35-34.34a8,8,0,0,0,0-11.32L59.31,64h160Z"/></g>"#;

/// A garage and its door.
/// Phosphor's `garage`.
const GARAGE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M184,136v64H72V136Z"/><path fill="currentColor" d="M240,192h-8V98.67a16,16,0,0,0-7.12-13.31l-88-58.67a16,16,0,0,0-17.75,0l-88,58.67A16,16,0,0,0,24,98.67V192H16a8,8,0,0,0,0,16H240a8,8,0,0,0,0-16ZM40,98.67,128,40l88,58.66V192H192V136a8,8,0,0,0-8-8H72a8,8,0,0,0-8,8v56H40ZM176,144v16H136V144Zm-56,16H80V144h40ZM80,176h40v16H80Zm56,0h40v16H136Z"/></g>"#;

/// A hard hat.
/// Phosphor's `hard-hat`.
const HARD_HAT: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M104,51.31V160H40V136A88,88,0,0,1,104,51.31Zm48,0V160h64V136A88,88,0,0,0,152,51.31Z"/><path fill="currentColor" d="M224,152V136a96.37,96.37,0,0,0-64-90.51V40a16,16,0,0,0-16-16H112A16,16,0,0,0,96,40v5.49A96.37,96.37,0,0,0,32,136v16a16,16,0,0,0-16,16v24a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V168A16,16,0,0,0,224,152Zm-16-16v16H160V62.67A80.36,80.36,0,0,1,208,136ZM144,40V152H112V40ZM48,136A80.36,80.36,0,0,1,96,62.67V152H48Zm176,56H32V168H224v24Z"/></g>"#;

/// Code brackets.
/// Phosphor's `code`.
const CODE: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M240,128l-48,40H64L16,128,64,88H192Z"/><path fill="currentColor" d="M69.12,94.15,28.5,128l40.62,33.85a8,8,0,1,1-10.24,12.29l-48-40a8,8,0,0,1,0-12.29l48-40a8,8,0,0,1,10.24,12.3Zm176,27.7-48-40a8,8,0,1,0-10.24,12.3L227.5,128l-40.62,33.85a8,8,0,1,0,10.24,12.29l48-40a8,8,0,0,0,0-12.29ZM162.73,32.48a8,8,0,0,0-10.25,4.79l-64,176a8,8,0,0,0,4.79,10.26A8.14,8.14,0,0,0,96,224a8,8,0,0,0,7.52-5.27l64-176A8,8,0,0,0,162.73,32.48Z"/></g>"#;

/// A price tag: also a kept listing's price dropped (notifications.rs).
/// Phosphor's `tag`.
pub const PRICE_TAG: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M237.66,153,153,237.66a8,8,0,0,1-11.31,0L42.34,138.34A8,8,0,0,1,40,132.69V40h92.69a8,8,0,0,1,5.65,2.34l99.32,99.32A8,8,0,0,1,237.66,153Z"/><path fill="currentColor" d="M243.31,136,144,36.69A15.86,15.86,0,0,0,132.69,32H40a8,8,0,0,0-8,8v92.69A15.86,15.86,0,0,0,36.69,144L136,243.31a16,16,0,0,0,22.63,0l84.68-84.68a16,16,0,0,0,0-22.63Zm-96,96L48,132.69V48h84.69L232,147.31ZM96,84A12,12,0,1,1,84,72,12,12,0,0,1,96,84Z"/></g>"#;

/// A steering wheel.
/// Phosphor's `steering-wheel`.
const STEERING_WHEEL: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M222.4,145.55A96.6,96.6,0,0,1,218.54,160h-53a8,8,0,0,0-7.5,5.19l-21.9,58.47c-2.69.22-5.41.34-8.15.34s-5.5-.12-8.2-.35L97.94,165.2a8,8,0,0,0-7.49-5.2h-53a96.6,96.6,0,0,1-3.86-14.45,128,128,0,0,1,188.8,0Z"/><path fill="currentColor" d="M128,152a12,12,0,1,1,12-12A12,12,0,0,1,128,152Zm104-24A104,104,0,1,1,128,24,104.11,104.11,0,0,1,232,128ZM40,128v.33a135.93,135.93,0,0,1,176,0V128a88,88,0,0,0-176,0Zm67.5,85.58L90.45,168H49.63A88.35,88.35,0,0,0,107.5,213.58ZM128,216c.83,0,1.66,0,2.49,0l20.07-53.57a16.07,16.07,0,0,1,15-10.39h47.12c.38-1.31.72-2.64,1-4a120,120,0,0,0-171.4,0c.31,1.34.65,2.67,1,4H90.45a16.08,16.08,0,0,1,15,10.4l20,53.56C126.31,216,127.15,216,128,216Zm78.37-48H165.55l-17.09,45.59A88.34,88.34,0,0,0,206.37,168Z"/></g>"#;

/// A headset and its microphone.
/// Phosphor's `headset`.
const HEADSET: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M80,144v40a16,16,0,0,1-16,16H48a16,16,0,0,1-16-16V128H64A16,16,0,0,1,80,144Zm112-16a16,16,0,0,0-16,16v40a16,16,0,0,0,16,16h32V128Z"/><path fill="currentColor" d="M201.89,54.66A104.08,104.08,0,0,0,24,128v56a24,24,0,0,0,24,24H64a24,24,0,0,0,24-24V144a24,24,0,0,0-24-24H40.36A88.12,88.12,0,0,1,190.54,65.93,87.39,87.39,0,0,1,215.65,120H192a24,24,0,0,0-24,24v40a24,24,0,0,0,24,24h24a24,24,0,0,1-24,24H136a8,8,0,0,0,0,16h56a40,40,0,0,0,40-40V128A103.41,103.41,0,0,0,201.89,54.66ZM64,136a8,8,0,0,1,8,8v40a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V136Zm128,56a8,8,0,0,1-8-8V144a8,8,0,0,1,8-8h24v56Z"/></g>"#;

/// A screwdriver.
/// Phosphor's `screwdriver`.
const SCREWDRIVER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M227.57,88.43l-53.23,53.23a8,8,0,0,1-5.65,2.34H152a8,8,0,0,0-8,8v16.83a8,8,0,0,1-1.62,4.83l-8,8a8,8,0,0,1-11.32,0L74.34,132.94a8,8,0,0,1,0-11.32l8-8A8,8,0,0,1,87.17,112H104a8,8,0,0,0,8-8V87.31a8,8,0,0,1,2.34-5.65l53.23-53.23a42.42,42.42,0,0,1,60,0h0A42.42,42.42,0,0,1,227.57,88.43Z"/><path fill="currentColor" d="M205.66,50.32a8,8,0,0,1,0,11.32l-56,56a8,8,0,0,1-11.32-11.31l56-56A8,8,0,0,1,205.66,50.32ZM248,58.41a50.13,50.13,0,0,1-14.77,35.66L180,147.3A15.86,15.86,0,0,1,168.69,152H152v16.83a16,16,0,0,1-3.25,9.66,8.08,8.08,0,0,1-.72.83l-8,8a16,16,0,0,1-22.62,0L98.7,168.6l-77,77.06a8,8,0,0,1-11.32-11.32l77.05-77.05-18.7-18.71a16,16,0,0,1,0-22.63l8-8a8,8,0,0,1,.82-.72A16.14,16.14,0,0,1,87.17,104H104V87.3A15.92,15.92,0,0,1,108.68,76l53.24-53.23A50.43,50.43,0,0,1,248,58.41Zm-16,0a34.43,34.43,0,0,0-58.77-24.35L120,87.3V104a16,16,0,0,1-16,16H87.28L80,127.27,128.72,176l7.28-7.28V152a16,16,0,0,1,16-16h16.69l53.23-53.24A34.21,34.21,0,0,0,232,58.41Z"/></g>"#;

/// A broom.
/// Phosphor's `broom`.
const BROOM: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M192.8,165.12,43.93,105.57A110.88,110.88,0,0,1,61.47,82.38a8,8,0,0,1,8.67-1.81L95.52,90.85a16,16,0,0,0,20.82-9l21-53.1c4.15-10,15.47-15.33,25.63-11.53a20,20,0,0,1,11.51,26.39L153.13,96.71a16,16,0,0,0,8.93,20.75L187,127.3a8,8,0,0,1,5,7.43V152A104.58,104.58,0,0,0,192.8,165.12Z"/><path fill="currentColor" d="M235.5,216.81c-22.56-11-35.5-34.58-35.5-64.8V134.73a15.94,15.94,0,0,0-10.09-14.87L165,110a8,8,0,0,1-4.48-10.34l21.32-53a28,28,0,0,0-16.1-37,28.14,28.14,0,0,0-35.82,16,.61.61,0,0,0,0,.12L108.9,79a8,8,0,0,1-10.37,4.49L73.11,73.14A15.89,15.89,0,0,0,55.74,76.8C34.68,98.45,24,123.75,24,152a111.45,111.45,0,0,0,31.18,77.53A8,8,0,0,0,61,232H232a8,8,0,0,0,3.5-15.19ZM67.14,88l25.41,10.3a24,24,0,0,0,31.23-13.45l21-53c2.56-6.11,9.47-9.27,15.43-7a12,12,0,0,1,6.88,15.92L145.69,93.76a24,24,0,0,0,13.43,31.14L184,134.73V152c0,.33,0,.66,0,1L55.77,101.71A108.84,108.84,0,0,1,67.14,88Zm48,128a87.53,87.53,0,0,1-24.34-42,8,8,0,0,0-15.49,4,105.16,105.16,0,0,0,18.36,38H64.44A95.54,95.54,0,0,1,40,152a85.9,85.9,0,0,1,7.73-36.29l137.8,55.12c3,18,10.56,33.48,21.89,45.16Z"/></g>"#;

/// A video camera.
/// Phosphor's `video-camera`.
const VIDEO_CAMERA: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M200,72V184a8,8,0,0,1-8,8H32a8,8,0,0,1-8-8V72a8,8,0,0,1,8-8H192A8,8,0,0,1,200,72Z"/><path fill="currentColor" d="M251.77,73a8,8,0,0,0-8.21.39L208,97.05V72a16,16,0,0,0-16-16H32A16,16,0,0,0,16,72V184a16,16,0,0,0,16,16H192a16,16,0,0,0,16-16V159l35.56,23.71A8,8,0,0,0,248,184a8,8,0,0,0,8-8V80A8,8,0,0,0,251.77,73ZM192,184H32V72H192V184Zm48-22.95-32-21.33V116.28L240,95Z"/></g>"#;

/// A paint roller.
/// Phosphor's `paint-roller`.
const PAINT_ROLLER: &str = r#"<g transform="scale(.09375)" stroke="none"><path d="M208,64v64a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H200A8,8,0,0,1,208,64Z"/><path fill="currentColor" d="M232,88H216V64a16,16,0,0,0-16-16H48A16,16,0,0,0,32,64V88H16a8,8,0,0,0,0,16H32v24a16,16,0,0,0,16,16H200a16,16,0,0,0,16-16V104h16v50L131.6,182.65A16.07,16.07,0,0,0,120,198v34a8,8,0,0,0,16,0V198l100.4-28.68A16.07,16.07,0,0,0,248,154V104A16,16,0,0,0,232,88Zm-32,40H48V64H200v64Z"/></g>"#;

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
    ("makina", CAR_SIDE),
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
