pub mod art;
pub mod book;
pub mod clip;
pub mod draw;
pub mod edit;
pub mod symbol;
pub mod generate;
pub mod guides;
pub mod imaging;
pub mod inks;
pub mod paths;
pub mod more;
pub mod prod;
pub mod refs;
pub mod file;
pub mod find;
pub mod layout;
pub mod object;
pub mod review;
pub mod swatch;
pub mod table;
pub mod text;
pub mod xml;

use crate::registry::Registry;

pub fn registry() -> Registry {
    let mut r = Registry::default();
    edit::register(&mut r);
    object::register(&mut r);
    text::register(&mut r);
    layout::register(&mut r);
    file::register(&mut r);
    xml::register(&mut r);
    swatch::register(&mut r);
    review::register(&mut r);
    table::register(&mut r);
    find::register(&mut r);
    book::register(&mut r);
    clip::register(&mut r);
    guides::register(&mut r);
    paths::register(&mut r);
    refs::register(&mut r);
    generate::register(&mut r);
    art::register(&mut r);
    draw::register(&mut r);
    symbol::register(&mut r);
    prod::register(&mut r);
    inks::register(&mut r);
    more::register(&mut r);
    imaging::register(&mut r);
    r
}
