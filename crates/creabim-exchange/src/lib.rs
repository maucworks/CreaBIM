//! Working alongside other tools. Objects read from a Blender (and, through
//! the same path, FreeCAD or IFC) file become elements that remember where
//! they came from, so reloading the file updates them in place, keeps the
//! names, classes and materials given in CreaBIM, and keeps an object that
//! was turned into a wall a wall. Walls, slabs, doors and windows exported
//! to glTF carry their ids, so edits made to them in Blender come back as
//! edits to the same elements.

pub mod assign;
pub mod blend;
pub mod catalogue;
pub mod freecad;
pub mod gltf;
pub mod gltf_read;
pub mod gpkg;
pub mod link;
pub mod materials;
pub mod shape;
pub mod site;
pub mod titleblock;

pub use assign::{assign, Assign};
pub use link::{sync, Incoming, SyncReport};
pub use site::{sync_site, SiteReport};

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error(transparent)]
    Blend(#[from] creabim_blend::BlendError),
    #[error(transparent)]
    Model(#[from] creabim_core::ModelError),
    #[error("{0}")]
    Other(String),
}
