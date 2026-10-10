pub mod book;
pub mod clip;
pub mod edit;
pub mod guides;
pub mod paths;
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
    r
}
