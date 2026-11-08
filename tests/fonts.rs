//! The bundled fonts are real font files that fontdb can load under the
//! family names the card template refers to.

use fontdb::{Database, Family, Query};

const FONTS: [&[u8]; 3] = [
    include_bytes!("../fonts/IBMPlexMono-Medium.ttf"),
    include_bytes!("../fonts/Fraunces-Variable.ttf"),
    include_bytes!("../fonts/BebasNeue-Regular.ttf"),
];

fn bundled_fonts() -> Database {
    let mut db = Database::new();
    for data in FONTS {
        db.load_font_data(data.to_vec());
    }
    db
}

fn has_family(db: &Database, name: &str) -> bool {
    db.query(&Query {
        families: &[Family::Name(name)],
        ..Query::default()
    })
    .is_some()
}

#[test]
fn bundled_fonts_provide_the_template_families() {
    let db = bundled_fonts();
    for family in ["IBM Plex Mono", "Fraunces", "Bebas Neue"] {
        assert!(has_family(&db, family), "font family {family:?} not found");
    }
}
